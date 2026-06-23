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
pub(crate) fn setup_editor_focus_gate(
    window: &ApplicationWindow,
    format_box: &gtk::Box,
    find_bar: &gtk::Revealer,
) {
    let format_w: gtk::Widget = format_box.clone().upcast();
    let find_w: gtk::Widget = find_bar.clone().upcast();
    let win_weak = window.downgrade();
    window.connect_focus_widget_notify(move |win| {
        let Some(focus) = GtkWindowExt::focus(win) else {
            return;
        };
        // Resolved fresh on every focus change: each tab
        // has its own editor widget, and only the ACTIVE tab's is meaningful —
        // a fixed captured widget would gate on whichever tab happened to be
        // active when the window was first created.
        let Some(editor_w) = win_weak
            .upgrade()
            .and_then(|w| state(&w))
            .map(|st| st.editor.clone().upcast::<gtk::Widget>())
        else {
            return;
        };
        let within = |anc: &gtk::Widget| focus == *anc || focus.is_ancestor(anc);
        // Transient surfaces leave the gate untouched (sticky): the Format toolbar
        // itself, the find bar, and any menu/popover.  Opening the menubar's Format
        // menu moves focus into a GtkPopoverMenu(Bar) — without this, the gate would
        // disable win.format right as the menu appears, greying every Format item.
        // The FIND BAR is sticky for the same reason: navigating to a find
        // match selects it in the editor and pops the caret overlay, but focus is in
        // the find entry — without this the gate would disable win.format and grey
        // every overlay action. In preview mode the gate is already disabled (the
        // editor was never focused), so find there stays correctly disabled.
        if within(&format_w)
            || within(&find_w)
            || focus.ancestor(gtk::PopoverMenuBar::static_type()).is_some()
            || focus.ancestor(gtk::PopoverMenu::static_type()).is_some()
            || focus.ancestor(gtk::Popover::static_type()).is_some()
        {
            return;
        }
        let enabled = within(&editor_w);
        if let Some(w) = win_weak.upgrade() {
            set_action_enabled(&w, "format", enabled);
            set_action_enabled(&w, "go-to-line", enabled);
            // Focus genuinely left the editor (and not into the popover, which is
            // parented inside the editor subtree) → dismiss the caret overlay too.
            if !enabled {
                if let Some(st) = state(&w) {
                    st.chrome().format_overlay.popdown();
                }
            }
            // Split-pane focus tracking for win.copy / win.select-all (TDD 9.25):
            // this point is past the transient-surface early-return, so a
            // popover/menu/find-bar that steals focus can't flip the pane. Record
            // which real pane holds focus, then re-evaluate Copy for its selection.
            let preview_focused =
                preview_text_view(&w).is_some_and(|pv| within(pv.upcast_ref::<gtk::Widget>()));
            if enabled || preview_focused {
                if let Some(ch) = crate::winstate::chrome(&w) {
                    ch.focused_pane.set(if preview_focused {
                        crate::winstate::FocusedPane::Preview
                    } else {
                        crate::winstate::FocusedPane::Editor
                    });
                }
                recompute_copy_enabled(&w);
            }
        }
    });
}
