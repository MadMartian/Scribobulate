//! The sticky editor-focus gate: enables/disables the editor-only, focus-dependent
//! actions (`win.format`, `win.go-to-line`) and tracks the split focused pane, moving
//! only when focus genuinely enters/leaves a real pane (never on a transient popover).

use super::super::*;

/// Gate every editor-only, focus-dependent action ("the editor is the active
/// edit target" — currently `win.format` and `win.go-to-line`) on
/// the window's focus-widget, NOT per-widget focus (which flickers and would
/// desensitise the heading combo mid-popup). The flag is sticky: a *null*
/// focus widget (a transient popover such as the combo's own popup owns focus) or
/// focus landing inside the Format toolbar leaves the gate untouched, so the
/// command surface never disables itself while the user is operating it.  Only
/// focus genuinely entering the editor (enable) or some other area such as the
/// preview pane or find bar (disable) moves the gate. Every gated action shares
/// this one function (single source of truth for "needs editor focus") rather
/// than each growing its own parallel focus-tracking closure.
///
/// A focus change is not the gate's only input: it also reads which tab is
/// active, and a tab switch moves the focus BEFORE it changes the active tab
/// (see [`sync_gate_to_pane_focus`]), so the switch re-evaluates the gate once
/// it has settled.
pub(crate) fn setup_editor_focus_gate(
    window: &ApplicationWindow,
    format_items: &[gtk::Widget],
    find_bar: &gtk::Revealer,
) {
    // A LIST, not one container. The Format commands are packed individually
    // into the toolbar's wrap box so a narrow window can wrap them (they were
    // one ~555px box, which set the window's whole minimum width), so there is
    // no single ancestor left to test. Membership against the list asks the
    // same question directly, and cannot go stale the way an ancestor test
    // does when the packing changes underneath it.
    let format_ws: Vec<gtk::Widget> = format_items.to_vec();
    let find_w: gtk::Widget = find_bar.clone().upcast();
    window.connect_focus_widget_notify(move |win| {
        let Some(focus) = GtkWindowExt::focus(win) else {
            return;
        };
        // Transient surfaces leave the gate untouched (sticky): the Format toolbar
        // itself, the find bar, and any menu/popover.  Opening the menubar's Format
        // menu moves focus into a GtkPopoverMenu(Bar) — without this, the gate would
        // disable win.format right as the menu appears, greying every Format item.
        // The FIND BAR is sticky for the same reason: navigating to a find
        // match selects it in the editor and pops the caret overlay, but focus is in
        // the find entry — without this the gate would disable win.format and grey
        // every overlay action. In preview mode the gate is already disabled (the
        // editor was never focused), so find there stays correctly disabled.
        if format_ws.iter().any(|anc| within(&focus, anc))
            || within(&focus, &find_w)
            || focus.ancestor(gtk::PopoverMenuBar::static_type()).is_some()
            || focus.ancestor(gtk::PopoverMenu::static_type()).is_some()
            || focus.ancestor(gtk::Popover::static_type()).is_some()
            || focus
                .ancestor(crate::widgets::floating_card::FloatingCard::static_type())
                .is_some()
        {
            return;
        }
        // Past the transient-surface early-return, so a popover/menu/find-bar that
        // steals focus can't flip the gate or the split pane (TDD 9.25). Focus in
        // neither pane is some other real area (the sidebar, the tab strip): the
        // editor is no longer the edit target.
        if !sync_gate_to_pane_focus(win) {
            set_gate(win, false);
        }
    });
}

/// Re-derive the gate, and the split focused pane, from the pane of the ACTIVE tab
/// that holds the keyboard focus. Returns `false`, changing nothing, when the focus
/// is in neither of that tab's panes.
///
/// Called on every focus change and again from the tab-switch resync
/// (`tabs::switch::resync_tab_action_state`). The second call is required: the tab
/// stack hands the focus to the incoming page's last-focused widget inside the
/// switch (`gtk_stack_set_visible_child`, GTK 4.6.9 `gtkstack.c:1333-1338`), before
/// the switch callback makes that tab the active one, so the focus change is judged
/// against the outgoing tab's panes and closes the gate with the editor focused.
/// The focus then does not move again, and a click on the editor that already has
/// it notifies nothing, so without the re-evaluation Format stayed off until the
/// focus left the editor and came back.
pub(crate) fn sync_gate_to_pane_focus(window: &ApplicationWindow) -> bool {
    let Some(focus) = GtkWindowExt::focus(window) else {
        return false;
    };
    // Resolved fresh every time: each tab has its own editor widget, and only the
    // ACTIVE tab's is meaningful.
    let Some(st) = state(window) else {
        return false;
    };
    let in_editor = within(&focus, st.editor.upcast_ref());
    let in_preview =
        !in_editor && preview_text_view(window).is_some_and(|pv| within(&focus, pv.upcast_ref()));
    if !in_editor && !in_preview {
        return false;
    }
    // A hidden editor is never the edit target, whatever GTK left the focus on;
    // `apply_mode_action_state` closes the gate for that case and this must not
    // reopen it.
    set_gate(
        window,
        in_editor && current_mode(window).is_editor_visible(),
    );
    // Record which real pane holds focus for win.copy / win.select-all (TDD 9.25),
    // then re-evaluate Copy for its selection.
    let pane = if in_preview {
        crate::winstate::FocusedPane::Preview
    } else {
        crate::winstate::FocusedPane::Editor
    };
    let moved =
        crate::winstate::chrome(window).is_some_and(|ch| ch.focused_pane.replace(pane) != pane);
    recompute_copy_enabled(window);
    // Find follows the same record in split mode (TDD 11.19), so a move to the other
    // pane moves the open search with it.
    if moved && current_mode(window) == crate::winstate::ViewMode::Split {
        crate::window::findbar::retarget_find_to_focused_pane(window);
    }
    true
}

/// Re-derive the gate for a tab switch that left the focus on a sticky surface (the menu
/// bar after any menu-bar command): neither pane holds it, so [`sync_gate_to_pane_focus`]
/// changed nothing, yet the switch's per-tab mode reset may have shut the gate for a tab
/// that showed no editor. The gate's last real decision still describes the reader, so it
/// is read back, for the tab now shown (TDD 10.6a). Not itself a decision, so it records
/// nothing.
pub(crate) fn restore_remembered_gate(window: &ApplicationWindow) {
    let remembered =
        crate::winstate::chrome(window).is_some_and(|ch| ch.editor_gate_remembered.get());
    apply_gate(
        window,
        crate::winstate::editor_gate_after_switch(current_mode(window), remembered),
    );
}

/// `focus` is `anc` or inside it.
fn within(focus: &gtk::Widget, anc: &gtk::Widget) -> bool {
    focus == anc || focus.is_ancestor(anc)
}

/// Open or close the gate for every action it governs. Closing it also dismisses
/// the caret overlay: focus genuinely left the editor (and not into the overlay card,
/// which the transient-surface check above treats as still editing).
fn set_gate(window: &ApplicationWindow, enabled: bool) {
    // A real decision: remember it for `restore_remembered_gate`.
    if let Some(ch) = crate::winstate::chrome(window) {
        ch.editor_gate_remembered.set(enabled);
    }
    apply_gate(window, enabled);
}

/// Write the gated actions' enabled state, and dismiss the caret overlay on close.
fn apply_gate(window: &ApplicationWindow, enabled: bool) {
    set_action_enabled(window, "format", enabled);
    set_action_enabled(window, "go-to-line", enabled);
    if !enabled {
        if let Some(st) = state(window) {
            crate::widgets::floating_card::FloatingCardExt::popdown(&st.chrome().format_overlay);
        }
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;
    use crate::window::testkit::test_app_suffixed;

    fn settle() {
        crate::testpump::drain_for(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_millis(250),
        );
    }

    fn format_enabled(win: &ApplicationWindow) -> bool {
        simple_action(win, "format")
            .expect("win.format")
            .is_enabled()
    }

    /// Switching tabs while the editor holds the focus leaves Format enabled when
    /// the editor of the tab switched to receives the focus.
    ///
    /// The tab stack hands the focus to the incoming page's last-focused widget
    /// INSIDE the switch, before the window's active tab is updated, so the focus
    /// gate's only evaluation compares the incoming editor against the outgoing
    /// tab's editor and closes. Nothing re-evaluated it once the switch settled,
    /// and a click on the editor that already holds the focus moves no focus, so
    /// Format stayed off until the focus left the editor and came back.
    #[gtktest::test]
    fn switching_back_to_a_tab_whose_editor_had_focus_keeps_format_enabled() {
        round_trip_keeps_format_enabled("fmtgate-tabswitch", false);
    }

    /// TDD 10.6a: the same round trip when a sticky surface holds the focus.
    ///
    /// Every menu-bar command (View > Swap Panes is the reported one) ends with the
    /// focus on the menu bar rather than back in the editor, and the gate is sticky
    /// there, so it stays open. A round trip through a tab that shows no editor then
    /// closed it (`apply_mode_action_state` forces it off for that tab) and nothing
    /// could reopen it: the focus was in no pane, and the tab stack hands the focus
    /// back only when the outgoing page held it. The Bold button stands in for the
    /// menu bar here; both are sticky surfaces the gate leaves alone.
    #[gtktest::test]
    fn switching_back_to_a_tab_keeps_format_enabled_while_a_sticky_surface_holds_focus() {
        round_trip_keeps_format_enabled("fmtgate-tabswitch-sticky", true);
    }

    /// The toolbar button for `win.format::<target>`, found by walking the live tree.
    fn find_format_button(w: &gtk::Widget, target: &str) -> Option<gtk::Widget> {
        if let Some(b) = w.downcast_ref::<gtk::Button>() {
            if b.action_name().as_deref() == Some("win.format")
                && b.action_target_value()
                    .and_then(|v| v.str().map(str::to_string))
                    .as_deref()
                    == Some(target)
            {
                return Some(w.clone());
            }
        }
        let mut c = w.first_child();
        while let Some(ch) = c {
            if let Some(found) = find_format_button(&ch, target) {
                return Some(found);
            }
            c = ch.next_sibling();
        }
        None
    }

    fn round_trip_keeps_format_enabled(suffix: &str, sticky_focus: bool) {
        let app = test_app_suffixed(suffix);
        let win = new_window(&app, "IT-fmtgate-tabs", "# A\n\nThe quick brown fox.", None);
        app.change_action_state(
            &crate::window::toolbarchrome::section_action("format"),
            &true.to_variant(),
        );
        change_action_state(&win, "view-mode", &"split".to_variant());
        win.present();
        settle();
        let tab_a = state(&win).expect("tab A");
        let chrome = crate::winstate::chrome(&win).expect("chrome");
        let tab_b_id = crate::window::create_tab_in_window(&win, "# B\n\nbody", None, false, false)
            .expect("tab B");
        settle();
        assert!(chrome.tabs.focus_page(&tab_a.content_box), "back on tab A");
        settle();

        // The reported state: split mode, text selected in tab A's editor, which
        // holds the focus. Cleared first so the grab is a real focus change: the
        // switch above may already have handed tab A's editor the focus, and
        // re-grabbing the focus widget notifies nothing.
        GtkWindowExt::set_focus(&win, None::<&gtk::Widget>);
        tab_a.editor.grab_focus();
        let buf = tab_a.editor.buffer();
        buf.select_range(&buf.iter_at_offset(4), &buf.iter_at_offset(13));
        settle();
        assert!(
            format_enabled(&win),
            "precondition: editor focus opens the gate"
        );

        if sticky_focus {
            // A menu-bar command leaves the focus on a sticky surface; the editor keeps
            // its selection. The gate must stay open through that.
            let bold = find_format_button(&win.child().expect("window child"), "bold")
                .expect("a win.format::bold button is somewhere in the toolbar");
            bold.grab_focus();
            settle();
            assert!(
                format_enabled(&win),
                "precondition: a sticky surface leaves the gate open"
            );
        }

        let tab_b = crate::winstate::tab_by_id(tab_b_id).expect("tab B registered");
        assert!(
            chrome.tabs.focus_page(&tab_b.content_box),
            "switch to tab B"
        );
        settle();
        assert!(
            chrome.tabs.focus_page(&tab_a.content_box),
            "switch back to tab A"
        );
        settle();

        let focus = GtkWindowExt::focus(&win);
        assert!(
            sticky_focus || focus.as_ref() == Some(tab_a.editor.upcast_ref::<gtk::Widget>()),
            "precondition: the switch back hands the focus to tab A's editor (got {:?})",
            focus.map(|f| f.type_().name())
        );
        assert!(
            format_enabled(&win),
            "tab A's editor holds the focus after the switch back, yet win.format is \
             disabled: the focus gate was evaluated against the outgoing tab's editor \
             and nothing re-evaluated it once the active tab changed"
        );
        win.destroy();
    }
}
