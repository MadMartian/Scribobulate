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
use crate::saferizer::owned_mark::OwnedMark;

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
    let find_history_btn = &chrome.find_history_btn;
    let replace_history_btn = &chrome.replace_history_btn;
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
                    rr.set_visible(replace_mode);
                    sync_replace_availability(&st);
                    // Show the match count label only if there is a search string.
                    mc.set_visible(!st.chrome().find_entry.text().is_empty());
                    // The engine lights the whole buffer, so only when no passage
                    // confines the search; a held passage is repainted once the bar is
                    // revealed, below.
                    // And only when the editor is the pane searched (TDD 11.19).
                    let unscoped = st.find_scope.borrow().is_none();
                    let editor_searched = matches!(find_target(&w), FindTarget::Editor);
                    st.search_context.set_highlight(unscoped && editor_searched);
                }
                fr.set_reveal_child(true);
                // Re-read the editor's count on every reopen that searches the
                // editor, scoped or not: in split the readout may still hold the
                // PREVIEW's count from the last search, and an unchanged query makes
                // the engine's count notification silent (TDD 11.19).
                if let Some(st) = state(&w) {
                    if !fe.text().is_empty() && matches!(find_target(&w), FindTarget::Editor) {
                        crate::window::find::update_editor_readout(
                            &st,
                            st.find_cursor.get().editor_index(),
                        );
                    }
                }
                // Select all text in the entry so the next keystroke replaces it.
                //
                // SELECT BEFORE FOCUSING, and do not reorder these two lines.
                // `gtk_text_focus_changed` starts the cursor-blink tick callback
                // whenever the entry takes focus with no selection, and NOTHING on
                // the selection path stops it again: `gtk_text_set_selection_bounds`
                // never calls `gtk_text_check_cursor_blink` (source-read at 4.6.9).
                // So focusing first and selecting second leaves a blink tick running
                // over a selection, and the next frame makes GTK notice its own
                // inconsistent state and log `GtkText - unexpected blinking
                // selection. Removing`. Selecting first means the focus-in check sees
                // the selection and never arms the tick — including the identical
                // select-all that `gtk_text_grab_focus` performs on our behalf when
                // `gtk-entry-select-on-focus` is set, which is why this order matters
                // even though the explicit call below looks redundant. Only a REOPEN
                // over a non-empty entry can reach it, which is why every isolated
                // leg of the first investigation came back clean.
                fe.select_region(0, -1);
                fe.grab_focus();
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
                resync_preview_find(&w, &st, &view, text.as_str());
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
                // The engine's highlight is a property; the scoped one is a tag on the
                // BUFFER, which outlives the bar unless it is taken off here.
                crate::window::find::clear_editor_scope_highlight(&st);
            }
            clear_preview_highlight(&w);
            // Return focus to the pane the search was acting on: the editor, or in split
            // mode the preview when that is the pane the reader was in (TDD 11.19). Pure
            // preview leaves the focus where it is.
            match find_target(&w) {
                FindTarget::Editor => {
                    if let Some(st) = state(&w) {
                        st.editor.grab_focus();
                    }
                }
                FindTarget::Preview(view) if current_mode(&w) == ViewMode::Split => {
                    view.grab_focus();
                }
                FindTarget::Preview(_) | FindTarget::PreviewUnresolved => {}
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

    // ── Escape anywhere in the window while the bar is open ───────────────────
    // The two handlers above only ever see Escape when focus is INSIDE the bar, and
    // focus usually is not: opening the bar, typing a query and pressing Enter moves
    // the caret into the document, and from then on Escape did nothing at all. The
    // reader's model is that Escape dismisses the thing that just appeared, and it was
    // only honoured while they happened to be standing in it.
    //
    // **BUBBLE phase, deliberately, and this is the whole safety argument.** A key
    // event is offered to the focused widget's controllers before it bubbles to the
    // window, so anything else with a claim on Escape answers first and stops it:
    // popovers and menus, the annotation comment card, the emoji overlay, the prompt
    // dialog and the annotations sidebar all take Escape this way. Capture phase would
    // put this handler AHEAD of every one of them and close the find bar instead of the
    // popover the reader was actually looking at — a strictly worse bug than the one
    // being fixed, and one that would look like the popover refusing to close.
    //
    // The reveal check is what keeps this inert the rest of the time: with the bar
    // closed the handler declines, so Escape still belongs to whatever else wants it.
    //
    // **`Stop` IS RETURNED ON EVIDENCE, NEVER ON THE ASSUMPTION THAT CLOSING WORKED.**
    // The obvious spelling — close, then return `Stop` — swallows the key for every
    // other consumer in the window, and it does so on the strength of a call whose
    // result was not checked. If the bar is ever still open afterwards, that spelling
    // enters the same branch on the next press, does nothing visible, and swallows
    // Escape again: the key is dead for the life of the process, and the reader sees an
    // open bar that will not close by any means. Re-reading the revealer and declining
    // when it did not close turns that permanent wedge into, at worst, the behaviour
    // this handler was added to improve on. A `mac` seat run did wedge exactly that way
    // once and was never reproduced, so this is a bound on the blast radius rather than
    // a fix for a diagnosed cause — do not read it as one (ISSUES).
    //
    // The predicate asks BOTH revealer properties for the same reason. `reveals_child`
    // is the target state and `child_revealed` is the drawn one, and they differ for the
    // length of the slide animation — and would differ indefinitely if the two ever
    // desynced, which is the other shape the wedge above could take. Taking either as
    // "the bar is up" means the handler fires whenever the reader can see a bar,
    // which is the only definition that matches what they are pressing Escape at.
    {
        let cfb = Rc::clone(&close_find_bar);
        let fr = find_bar_revealer.clone();
        let key_ctrl = gtk::EventControllerKey::new();
        key_ctrl.set_propagation_phase(gtk::PropagationPhase::Bubble);
        key_ctrl.connect_key_pressed(move |_, key, _, _| {
            let showing = |r: &gtk::Revealer| r.reveals_child() || r.is_child_revealed();
            if key != gtk::gdk::Key::Escape || !showing(&fr) {
                return glib::Propagation::Proceed;
            }
            cfb();
            if fr.reveals_child() {
                // It did not take. Decline, so Escape stays available to everything
                // else rather than being consumed by a handler that achieved nothing.
                log::warn!(
                    "find: Escape did not close the find bar; leaving the key to other handlers"
                );
                return glib::Propagation::Proceed;
            }
            glib::Propagation::Stop
        });
        window.add_controller(key_ctrl);
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
            record_committed_query(&st);
            find_step(&w, &st.search_context, SearchDir::Forward);
        }
    ));
    find_prev_btn.connect_clicked(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_| {
            let Some(st) = state(&w) else { return };
            record_committed_query(&st);
            find_step(&w, &st.search_context, SearchDir::Backward);
        }
    ));

    // ── Enter / Shift+Enter in find_entry ────────────────────────────────────
    find_entry.connect_activate(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_| {
            let Some(st) = state(&w) else { return };
            record_committed_query(&st);
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
                        record_committed_query(&st);
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
    find_entry.connect_search_changed(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |entry| {
            let Some(st) = state(&w) else { return };
            // Remember this tab's own query (operator decision Q13) so switching
            // away and back repopulates it rather than showing another tab's term.
            *st.find_query.borrow_mut() = entry.text().to_string();
            refresh_find(&w, &st);
        }
    ));

    // ── win.find-match-case / win.find-whole-word / win.find-regex ────────────
    // The three match options, one stateful boolean action each. **Uncommon commands**
    // (CAM § Uncommon commands): a find-bar toggle and nothing else — no menu item,
    // toolbar section or context-menu entry — because they qualify a query that only
    // exists while the bar is open.
    //
    // One action per option, which the find-bar toggle is bound to (POLICY "One action
    // per command"). The option itself lives on the TAB, not on the action: the action's
    // state is a mirror of it, resynced on a tab switch exactly as `show-unsafe-images`
    // is.
    for (name, accessor) in FindOptions::ACTIONS {
        super::viewactions::register_bool_action(window, name, false, move |window, on| {
            let Some(st) = state(window) else { return };
            let mut options = st.find_options.get();
            accessor.set(&mut options, on);
            st.find_options.set(options);
            refresh_find(window, &st);
        });
    }

    // ── win.find-in-selection ────────────────────────────────────────────────
    // The fourth toggle, and the one that is NOT a match option (`FindOptions` says
    // why): it does not change what counts as a match, it bounds where matching is
    // applied — and it is a held reference into the document rather than a boolean, so
    // it is neither persisted nor carried in the same struct.
    //
    // It scopes FINDING. Replace is then an action on what finding produced, which is
    // why this is not a replace feature and is not editor-only.
    let in_selection =
        super::viewactions::register_bool_action(window, FIND_IN_SELECTION, false, |window, on| {
            let Some(st) = state(window) else { return };
            if on {
                capture_find_scope(window, &st);
            } else {
                *st.find_scope.borrow_mut() = None;
            }
            refresh_find(window, &st);
        });
    // Nothing is selected in a freshly built window, so the control starts unavailable
    // and the selection wiring raises it.
    in_selection.set_enabled(false);

    // ── The two history drop-downs ───────────────────────────────────────────
    // Each field's recent entries, most recent first. **Built on demand**, not resynced:
    // the buttons are the WINDOW's and the histories are the TAB's, so a model built
    // once and refreshed on a tab switch is one more mirror to keep true. A
    // `create_popup_func` runs at the moment the reader presses the button, which makes
    // "the active tab's own list" true by construction rather than by upkeep.
    for (btn, field) in [
        (find_history_btn, HistoryField::Find),
        (replace_history_btn, HistoryField::Replace),
    ] {
        btn.set_create_popup_func(glib::clone!(
            #[weak(rename_to = w)]
            window,
            move |btn| {
                btn.set_menu_model(state(&w).map(|st| build_history_menu(&st, field)).as_ref());
            }
        ));
    }

    // One parameterised action for both drop-downs' rows: the chosen text is the
    // target, so a row is not a closure and the menu can be rebuilt freely.
    let pick = SimpleAction::new(PICK_HISTORY, Some(glib::VariantTy::STRING));
    pick.connect_activate(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_, value| {
            let Some(chosen) = value.and_then(|v| v.get::<String>()) else {
                return;
            };
            let Some((field, text)) = HistoryField::split_target(&chosen) else {
                log::error!("find: a history row carried an unreadable target");
                return;
            };
            let Some(st) = state(&w) else { return };
            let chrome = st.chrome();
            match field {
                // Choosing a search term searches for it immediately — the reader
                // opened the list to get back to a search, not to fill a box.
                HistoryField::Find => {
                    chrome.find_entry.set_text(text);
                    record_committed_query(&st);
                    refresh_find(&w, &st);
                }
                // A replacement is not an action on its own; it fills the field and
                // waits, exactly as typing it would.
                HistoryField::Replace => chrome.replace_entry.set_text(text),
            }
        }
    ));
    window.add_action(&pick);

    // ── Replace button ────────────────────────────────────────────────────────
    let re = replace_entry.clone();
    replace_btn.connect_clicked(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_| {
            let Some(st) = state(&w) else { return };
            record_committed_query(&st);
            record_committed_replacement(&st);
            replace_current_match(&w, &st, re.text().as_str());
        }
    ));

    // ── Replace All button ────────────────────────────────────────────────────
    let re = replace_entry.clone();
    replace_all_btn.connect_clicked(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_| {
            let Some(st) = state(&w) else { return };
            record_committed_query(&st);
            record_committed_replacement(&st);
            replace_all_matches(&w, &st, re.text().as_str());
        }
    ));
}

/// The `win.` action a history row activates, carrying the chosen text as its target.
///
/// One action for both drop-downs rather than one each: a row's payload is the text,
/// and which field it came from is a prefix on that text rather than a second action
/// name to keep in step with a second menu builder.
pub(crate) const PICK_HISTORY: &str = "pick-find-history";

/// Which field a history belongs to.
///
/// The `win.` action's target has to say, because one action serves both drop-downs and
/// a bare string cannot. Encoded as a one-character prefix on the text rather than as a
/// tuple variant: a `GAction` target is a `GVariant`, a string is the cheapest shape
/// that survives a menu model, and the entries themselves are arbitrary text so no
/// separator is safe unless it is at a FIXED position.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum HistoryField {
    Find,
    Replace,
}

impl HistoryField {
    /// This field's marker character. Never a separator to split on — the entry may
    /// contain anything — only the first character of the target.
    fn marker(self) -> char {
        match self {
            HistoryField::Find => 'f',
            HistoryField::Replace => 'r',
        }
    }

    /// The action target for `entry` in this field.
    fn target(self, entry: &str) -> String {
        format!("{}{entry}", self.marker())
    }

    /// Read a target back. `None` for anything that does not begin with a known marker,
    /// which is a target this application did not write.
    fn split_target(target: &str) -> Option<(Self, &str)> {
        let mut chars = target.chars();
        let field = match chars.next()? {
            'f' => HistoryField::Find,
            'r' => HistoryField::Replace,
            _ => return None,
        };
        Some((field, chars.as_str()))
    }

    /// This field's history on `st`, cloned out — never borrowed across a caller that
    /// then touches GTK (GTK4Rs/AP-61).
    fn history(self, st: &Rc<TabState>) -> crate::window::FindHistory {
        match self {
            HistoryField::Find => st.find_history.borrow().clone(),
            HistoryField::Replace => st.replace_history.borrow().clone(),
        }
    }
}

/// Build the drop-down's model from `st`'s own history for `field`.
///
/// Rows are labelled by [`crate::window::row_label`] — shortened and single-lined,
/// because an entry is a string the reader typed — while the action target carries the
/// entry VERBATIM, so choosing a shortened row still searches for the whole thing.
fn build_history_menu(st: &Rc<TabState>, field: HistoryField) -> gtk::gio::Menu {
    let menu = gtk::gio::Menu::new();
    for entry in field.history(st).entries() {
        let item = gtk::gio::MenuItem::new(Some(&crate::window::row_label(entry)), None);
        item.set_action_and_target_value(
            Some(&format!("win.{PICK_HISTORY}")),
            Some(&field.target(entry).to_variant()),
        );
        menu.append_item(&item);
    }
    menu
}

/// Record the find field's current text as a committed search term.
///
/// **Committed, not typed.** The find field searches as you type, so every prefix of
/// every query is a search that happened; a history fed from `search-changed` is the
/// reader's last query once per keystroke and pushes everything else off the end. The
/// commit points are the ones where the reader has said "this one": Enter, Next, Prev,
/// Replace, Replace All, and choosing an entry from the drop-down itself.
pub(super) fn record_committed_query(st: &Rc<TabState>) {
    let text = st.chrome().find_entry.text();
    st.find_history.borrow_mut().record(text.as_str());
    sync_history_buttons(st);
}

/// The replacement-field counterpart, recorded at the two points a replacement is
/// actually applied.
pub(super) fn record_committed_replacement(st: &Rc<TabState>) {
    let text = st.chrome().replace_entry.text();
    st.replace_history.borrow_mut().record(text.as_str());
    sync_history_buttons(st);
}

/// A drop-down with nothing to offer is insensitive. Called wherever a history changes
/// or a different tab's becomes the active one — a button that opens an empty menu is
/// worse than one that says it has nothing.
pub(super) fn sync_history_buttons(st: &Rc<TabState>) {
    let chrome = st.chrome();
    chrome
        .find_history_btn
        .set_sensitive(!st.find_history.borrow().is_empty());
    chrome
        .replace_history_btn
        .set_sensitive(!st.replace_history.borrow().is_empty());
}

/// The `win.` action carrying **search in selection**.
///
/// A named constant rather than a literal because four surfaces spell it — the action
/// registration, the find-bar toggle's `action-name`, the Edit-menu item and the
/// sensitivity update — and an action name is a string no compiler checks. The three
/// match options get the same treatment from `FindOptions::ACTIONS`; this one is not in
/// that table because it is not one of them.
pub(crate) const FIND_IN_SELECTION: &str = "find-in-selection";

/// Capture the passage the reader has selected in whichever pane they are looking at.
///
/// The two panes index different spaces and so capture differently — see `FindScope`.
/// Capturing nothing leaves the toggle on with no bound, which would read as "in
/// selection, and everything is in the selection"; the action is insensitive without a
/// selection precisely so that state is unreachable.
fn capture_find_scope(window: &ApplicationWindow, st: &Rc<TabState>) {
    let captured = match find_target(window) {
        FindTarget::Editor => st.editor_buf.selection_bounds().map(|(start, end)| {
            // LEFT gravity on the start and RIGHT on the end, so text inserted at
            // either boundary lands INSIDE the passage rather than escaping it. Replace
            // All inserts at exactly those boundaries when the first or last match is
            // flush with them.
            FindScope::Editor {
                start: OwnedMark::new(st.editor_buf.create_mark(None, &start, true)),
                end: OwnedMark::new(st.editor_buf.create_mark(None, &end, false)),
            }
        }),
        FindTarget::Preview(view) => view.buffer().selection_bounds().map(|(start, end)| {
            FindScope::Preview(PreviewScope {
                key: RenderKey {
                    view_serial: view.instance_serial(),
                    generation: view.render_generation(),
                },
                start: start.offset(),
                end: end.offset(),
            })
        }),
        FindTarget::PreviewUnresolved => None,
    };
    *st.find_scope.borrow_mut() = captured;
}

/// Drop the captured passage and untick the toggle, from a path that discovered the
/// bound no longer resolves.
///
/// `set_action_state`, never `change_state`: the handler would clear a bound this has
/// already cleared and re-run the search that is in the middle of discovering it — and
/// the caller is `find`'s own scope resolution, so that is a re-entrant search.
pub(super) fn release_find_scope(window: &ApplicationWindow, st: &Rc<TabState>) {
    *st.find_scope.borrow_mut() = None;
    set_action_state(window, FIND_IN_SELECTION, &false.to_variant());
    update_find_scope_sensitivity(window);
}

/// Whether the in-selection control is usable: there is a passage to capture, or one is
/// already captured so the reader can let it go.
///
/// **The one option whose availability depends on the document rather than the query**,
/// which is why it has a sensitivity rule at all and the other three do not. Called
/// from every place a selection can change — the same three hooks `win.copy` is driven
/// from, since a selection is a selection.
pub(crate) fn update_find_scope_sensitivity(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    let captured = st.find_scope.borrow().is_some();
    let selectable = match find_target(window) {
        FindTarget::Editor => st.editor_buf.has_selection(),
        FindTarget::Preview(view) => view.buffer().has_selection(),
        FindTarget::PreviewUnresolved => false,
    };
    let enabled = captured || selectable;
    set_action_enabled(window, FIND_IN_SELECTION, enabled);
    crate::a11y::describe(
        &st.chrome().find_entry,
        (!enabled).then_some("Select a passage to confine the search to it."),
    );
}

/// Re-run the active tab's search under its current query AND options, and put the
/// outcome in the readout.
///
/// **One function for two triggers that are the same event.** Typing in the field and
/// ticking *match case* both change what the reader is asking for; nothing downstream
/// can tell them apart, and the moment they were two code paths one of them was going to
/// forget to push the options onto the editor's engine — which shows up as the editor
/// pane ignoring a toggle the preview pane honours.
///
/// Pushes the query and the options onto `SearchSettings` unconditionally, even in
/// pure-preview mode, so a later switch to edit or split finds the editor's engine
/// already asking the same question rather than the last one it was told.
pub(super) fn refresh_find(window: &ApplicationWindow, st: &Rc<TabState>) {
    reconcile_find_scope(window, st);
    let chrome = st.chrome();
    let label = &chrome.match_count_label;
    let text = chrome.find_entry.text();
    push_options_to_engine(st);
    // **Who owns the engine's whole-buffer highlight.** `GtkSourceSearchContext` has no
    // bounded highlight, so while a passage confines the search this application paints
    // the in-scope matches itself and the engine's own must be off — otherwise the
    // readout says 2 while the document stays lit end to end, and the number is the half
    // that reads as broken. Raised HERE rather than in `find::update_editor_readout`,
    // which also runs from the engine's own asynchronous count notification and so can
    // fire after the bar has closed and deliberately turned the highlight off.
    // Computed out of the borrow first: the setter can re-enter (GTK4Rs/AP-61).
    // Only while the bar is open: a menu toggle of Match Case or Whole Word, and the
    // entry's delayed `search-changed` after a tab switch, both reach here with it closed.
    // And only while the editor is the pane searched: in split with the preview
    // focused, an editor lit end to end beside a preview counting its own matches reads
    // as two searches with two answers (TDD 11.19: the pane left keeps no highlight).
    let unscoped = st.find_scope.borrow().is_none();
    let open = crate::window::find::find_bar_open(st);
    let editor_searched = matches!(find_target(window), FindTarget::Editor);
    st.search_context
        .set_highlight(unscoped && open && editor_searched);
    // **The query the editor's engine is given is not always the query the reader
    // typed.** For a whole-word REGULAR EXPRESSION the application wraps it — see
    // `matcher::editor_pattern` — because GtkSourceView's own wrapper is ungrouped and
    // binds each `\b` to one branch of an alternation. Every other case passes through
    // unchanged.
    crate::window::find::configure_engine_query(
        &st.search_settings,
        text.as_str(),
        st.find_options.get(),
    );
    label.set_visible(!text.is_empty());
    // The step cursor indexes a list that has just been replaced, whichever pane owns
    // it, so the next Next/Prev starts from the top.
    st.find_cursor.set(FindCursor::None);
    // Pure-preview mode: highlight the preview buffer (the editor engine can't, and the
    // editor isn't visible). Otherwise the source context's occurrences-count
    // notification refreshes the label.
    match find_target(window) {
        FindTarget::Preview(view) if open => resync_preview_find(window, st, &view, text.as_str()),
        FindTarget::Preview(_) => {}
        FindTarget::Editor => update_editor_readout(st, 0),
        // Deliberately NOT the editor arm: in pure-preview mode the editor's
        // occurrence count describes a buffer the user cannot see, so showing
        // it would be a confidently wrong number rather than a missing one.
        FindTarget::PreviewUnresolved => set_match_label(label, 0, 0),
    }
}

/// Release a captured passage the pane now being searched cannot use.
///
/// **A scope belongs to the pane it was taken in.** The editor's is a pair of marks in
/// the source buffer and the preview's is a range in a rendering of it; neither means
/// anything in the other pane. Switching view mode changes which pane the search acts
/// on, and a held scope of the wrong kind then confines nothing — while the control goes
/// on saying it does.
///
/// Reported by the macOS seat: capture a passage in Preview, switch to Side by Side,
/// and "Search in selection" stayed ticked against a whole-document count. The count was truthful,
/// which is what made it a mirror defect rather than a wrong answer — and mirror
/// defects are the ones nobody notices until they have trusted one.
///
/// Released rather than remembered-and-restored: keeping it would mean a control that
/// is ticked but inert for as long as the reader is in the other pane, which says
/// something false for longer. Re-deriving is the same answer the unresolvable-preview
/// case already gives (CAM § Document-Reference row 16).
fn reconcile_find_scope(window: &ApplicationWindow, st: &Rc<TabState>) {
    // Decided under the borrow, acted on outside it: `release_find_scope` writes the
    // same cell and moves an action's state (GTK4Rs/AP-61).
    let usable = matches!(
        (find_target(window), st.find_scope.borrow().as_ref()),
        (_, None)
            | (FindTarget::Editor, Some(FindScope::Editor { .. }))
            | (FindTarget::Preview(_), Some(FindScope::Preview(_)))
    );
    if !usable {
        log::info!(
            "find: the captured passage belongs to the other pane; the search now \
             covers the whole of this one"
        );
        release_find_scope(window, st);
    }
}

/// Point the three option GActions at `st`'s own options, without re-entering their
/// handlers.
///
/// The options are the TAB's; the actions are the WINDOW's. Every path that makes a
/// different tab the active one therefore owes this — a tab switch, and a session
/// restore, which is a tab becoming active having never been switched to. Without it
/// the toggles and the Edit-menu ticks describe whichever tab set them last, which is
/// the `show-unsafe-images` lying-mirror defect (`window::tabs::switch`) in a second
/// place.
///
/// `set_state`, never `change_state`: this is a resync onto state that is already
/// correct, and running the handlers would re-run the search for a tab that may not
/// even have its preview built yet.
pub(super) fn adopt_find_options(window: &ApplicationWindow, st: &Rc<TabState>) {
    let options = st.find_options.get();
    for (name, accessor) in FindOptions::ACTIONS {
        set_action_state(window, name, &accessor.get(&options).to_variant());
    }
    // The scope is a tab's too, and its toggle is the window's, so it takes the same
    // resync — and its sensitivity, which the other three do not have, has to be
    // recomputed against whatever the newly active tab has selected.
    set_action_state(
        window,
        FIND_IN_SELECTION,
        &st.find_scope.borrow().is_some().to_variant(),
    );
    update_find_scope_sensitivity(window);
    push_options_to_engine(st);
}

/// Push this tab's options onto its own `GtkSourceSearchSettings`.
///
/// The editor's engine implements two of the three natively; the third — whole word
/// under a regular expression — it implements WRONGLY for an alternation, so the
/// application takes it over (`matcher::editor_pattern`). It is a separate function
/// from [`refresh_find`] because a tab can become active with the find bar CLOSED —
/// there is nothing to recount then, but the engine still has to be holding this tab's
/// options before the bar next opens.
fn push_options_to_engine(st: &Rc<TabState>) {
    crate::window::find::configure_engine_options(&st.search_settings, st.find_options.get());
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
/// [`tabs::resolve_tab_window`], plus the tab's own chrome inside
/// [`update_editor_readout`], on every fire targets whichever window this tab
/// currently belongs to.
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
        // The editor's count describes the editor, so it is written only while the
        // editor is the pane being searched — never in pure preview, and never in split
        // mode while the reader is in the preview (TDD 11.19).
        if !matches!(find_target(&w), FindTarget::Editor) {
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
        // This handler already returned above unless the editor is the visible pane, so
        // the editor's index is the right space to read — and asking for it by name means
        // a preview cursor can never be misreported here as an editor position.
        //
        // **Through the readout rather than straight to the label**: the engine's count
        // is the WHOLE buffer's, and this notification is the one that arrives *after*
        // the scan settles — so writing it here directly is a scoped search's count
        // being overwritten by the unscoped one a moment later, which is the failure
        // that is invisible in any assertion taken before the engine finishes.
        update_editor_readout(&st, st.find_cursor.get().editor_index());
    });
}

/// Make the replace row usable exactly when the pane being searched is the editor.
///
/// The preview is read-only (TDD 11.4), and in split mode the pane searched is the one
/// last focused (TDD 11.19), so the row follows that pane rather than the view mode —
/// replacing in the editor while the counter and highlights describe the preview would
/// edit matches the reader has not been shown. Decided from `st`'s own mode so a tab
/// switch can call it before the incoming tab is the active one.
pub(super) fn sync_replace_availability(st: &Rc<TabState>) {
    let chrome = st.chrome();
    let row = &chrome.replace_row;
    let pane = crate::winstate::find_pane(st.view_mode.get(), chrome.focused_pane.get());
    let available = pane == crate::winstate::FocusedPane::Editor;
    row.set_sensitive(available);
    let why = match st.view_mode.get() {
        ViewMode::Split => {
            "Replace is unavailable in the preview pane. Click the editor to replace."
        }
        ViewMode::Preview | ViewMode::Edit => "Replace is unavailable in preview mode.",
    };
    crate::a11y::describe(
        row,
        (st.find_replace_mode.get() && !available).then_some(why),
    );
}

/// The split pane that last held focus changed: move the open search onto it.
///
/// Called by the focus gate when the focus lands in the OTHER real pane of a split.
/// The pane left behind keeps no find decoration — the preview's highlights are taken
/// off when the search moves to the editor, and `refresh_find` turns the editor's
/// engine highlight off when it moves to the preview. Then the whole search is
/// re-run against the new pane, so the counter, highlights and next Next all describe
/// the pane the reader is now in (TDD 11.19).
pub(crate) fn retarget_find_to_focused_pane(window: &ApplicationWindow) {
    let Some(st) = state(window) else { return };
    sync_replace_availability(&st);
    if !crate::window::find::find_bar_open(&st) {
        return;
    }
    if matches!(find_target(window), FindTarget::Editor) {
        clear_preview_highlight(window);
    }
    refresh_find(window, &st);
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
    if chrome.find_entry.text().is_empty() {
        return;
    }
    // **Both directions, not only the one that rebuilds a buffer.** This used to act on
    // the Preview arm alone, on the reasoning that a mode switch builds a fresh preview
    // whose highlights are gone while the editor's engine is untouched. The engine is
    // untouched — and the READOUT is not: it was still showing the count of the pane
    // the reader just left. The two panes legitimately count differently (the preview
    // searches three texts, so an anchored pattern matches in each), so switching
    // Preview→Edit left a number on screen that was true of neither the query nor the
    // pane, and only the next Next or keystroke corrected it. Reported by the macOS
    // seat ratifying the match-options batch; the bug is one arm short, not one signal
    // missed, which is why the fix routes the whole refresh rather than adding a second
    // call beside it.
    //
    // **The query did not change, so the reader's place survives.** `refresh_find` starts
    // the search over, which every one of these boundaries used to inherit: a fold splice
    // dropped "28 of 37" to "37 matches" and the next Find Next went to the top (TDD
    // 11.12). The cursor is taken before and put back after, re-found in the new list.
    let held = st.find_cursor.get();
    refresh_find(window, &st);
    if let FindTarget::Preview(view) = find_target(window) {
        let query = chrome.find_entry.text();
        crate::window::find::carry_preview_cursor(window, &st, &view, query.as_str(), held);
    }
}
