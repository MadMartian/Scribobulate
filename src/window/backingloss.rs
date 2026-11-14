//! A document whose buffer has become the only copy of it: its file was deleted, or
//! emptied, on disk (TDD 3.4, 3.5, 3.6, 15.22, 16.16, 22.18).
//!
//! When a loss begins or ends is decided elsewhere — the monitor's `Deleted` arm, and
//! `winstate::external_change_action` for everything read back from disk. This module
//! applies the answer, so both losses reach every guard through one route: Save's
//! enablement, the ⚠ badge, the close prompt (`TabState::needs_close_prompt`), the
//! crash-recovery snapshot and the status bar's persistent line all re-derive here.

use super::*;
use crate::winstate::BackingLoss;

/// Record that `tab`'s buffer is now the only copy of its document.
///
/// A repeat of the loss already recorded changes nothing, so a backend that reports
/// one deletion twice re-derives nothing.
pub(crate) fn mark_backing_lost(tab: &Rc<TabState>, loss: BackingLoss) {
    // A recorded loss supersedes a pending "is it still blank, or still gone?" re-read.
    super::reload::cancel_backing_settle(tab);
    if tab.backing_loss.replace(Some(loss)) == Some(loss) {
        return;
    }
    log::info!(
        "tab {}: backing file lost ({loss:?}): {}",
        tab.id,
        path_for_log(tab)
    );
    refresh_guards(tab);
}

/// Retire `tab`'s loss — the file is back with the content last loaded or saved, or a
/// save or an explicit reload made the buffer and the file agree.
pub(crate) fn clear_backing_loss(tab: &Rc<TabState>) {
    let Some(loss) = tab.backing_loss.take() else {
        return;
    };
    log::info!(
        "tab {}: backing file restored after {loss:?}: {}",
        tab.id,
        path_for_log(tab)
    );
    refresh_guards(tab);
}

/// Re-derive everything that reads the loss. Tab-scoped where the guard is the tab's
/// (badge, snapshot) and window-scoped where it is the window's (Save, the status
/// line), resolving the window the tab lives in NOW, since a background tab can be
/// moved between windows.
fn refresh_guards(tab: &Rc<TabState>) {
    if let Some(window) = window_of_content_box(&tab.content_box) {
        update_save_action_state(&window);
        // The loss is part of the persistent status line for as long as it holds —
        // never a timed notice, which would expire while the file is still gone. The
        // line is composed from the ACTIVE tab, so a background loss shows the moment
        // its tab is switched to (Derived-view CAM row 5).
        refresh_dirty_status(&window);
        // A loss supersedes the acknowledgements ("File saved", "File reloaded") of
        // whatever came before it: they would sit over the persistent line and the
        // prompt below for their remaining seconds. Only for the tab in front — the
        // shared toast and status stack are about the active document.
        if state(&window).is_some_and(|a| a.id == tab.id && tab.backing_loss.get().is_some()) {
            tab.chrome().retract_info_notices();
        }
        // Save's own enablement was re-derived above, which is what the prompt's Save
        // button binds to — so the prompt is raised after it, never before, or it would
        // appear for a beat offering a control GTK still had greyed.
        sync_backing_loss_toast(&window);
    }
    badge_tab_label(tab);
    sync_tab_swap(tab);
}

fn path_for_log(tab: &TabState) -> String {
    tab.path
        .borrow()
        .as_ref()
        .map_or_else(|| "<untitled>".into(), |p| p.display().to_string())
}
