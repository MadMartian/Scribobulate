//! The Insert↔Edit relabel of this window's Link/Image format surfaces (toolbar +
//! caret-overlay tooltips + the Format menu) from the editor selection.

use super::super::*;
use super::*;

/// The edit-kind of the editor selection: `Some(Link)`/`Some(Image)` when it is
/// exactly one existing link/image, else `None` — drives the Insert↔Edit relabel.
fn selection_edit_kind(sel: &str) -> Option<FmtInsertKind> {
    if format::parse_link(sel).is_some() {
        Some(FmtInsertKind::Link)
    } else if format::parse_image(sel).is_some() {
        Some(FmtInsertKind::Image)
    } else {
        None
    }
}

/// Relabel this window's Link/Image format surfaces (toolbar + caret-overlay tooltips)
/// Insert↔Edit from the editor selection — but only while `win.format` is enabled (the
/// editor is the active edit target; preview mode / no editor focus shows "Insert"). The
/// `mark-set` trigger is chatty, so tooltips are repainted only on an actual transition.
pub(crate) fn update_format_edit_surfaces(window: &ApplicationWindow) {
    refresh_format_edit_surfaces(window, false);
}

/// Forced-repaint variant, for a tab switch (ScrAP-61).  The edit-surfaces cache
/// (`WindowChrome.fmt_edit_state`) and both button sets (toolbar + the single caret
/// overlay) are now window-scoped, so the cache holds whatever tab last painted the
/// shared surfaces — a plain dedup could leave a stale "Edit …" tooltip after the
/// switch.  Repaint unconditionally against the newly-active tab's selection.
pub(crate) fn resync_format_edit_surfaces(window: &ApplicationWindow) {
    refresh_format_edit_surfaces(window, true);
}

fn refresh_format_edit_surfaces(window: &ApplicationWindow, force: bool) {
    let Some(st) = state(window) else { return };
    let chrome = st.chrome();
    let format_enabled = window
        .lookup_action("format")
        .map(|a| a.is_enabled())
        .unwrap_or(false);
    let kind = if format_enabled {
        let (_, _, sel) = selection_range_and_text(&st.editor_buf);
        selection_edit_kind(&sel)
    } else {
        None
    };
    // The mark-set trigger is chatty, so tooltips are repainted only on an actual
    // transition — except `force` (a tab switch), which repaints regardless because
    // the shared cache reflects the tab just left, not the one switched to. `replace`
    // is always evaluated so the cache tracks the latest kind either way.
    if chrome.fmt_edit_state.replace(kind) != kind || force {
        for (btn_kind, btn) in chrome.fmt_edit_btns.iter().chain(&chrome.overlay_edit_btns) {
            crate::a11y::name(btn, btn_kind.label(Some(*btn_kind) == kind));
        }
    }
    // Each window now owns its Format menu (ScrAP-63), so relabel THIS
    // window's directly — the old `is_active()` gate existed only because a single
    // shared menu could reflect just one window's selection at a time.
    crate::app::update_format_menu_labels(window, kind);
}
