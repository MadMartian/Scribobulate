//! Window-scoped tab-lifecycle `win.*` action registration: Close Tab, New
//! Window, Move Tab to New Window,
//! Previous/Next Tab, and the View ▸ Documents fast-switch radio, plus the
//! wrap-around tab cycling those actions drive. Split out of the former
//! monolithic `window/tabs.rs`.

use super::super::*;
use super::*;

/// Register the window-scoped tab-lifecycle `win.*` actions: Close Tab, New
/// Window, Move Tab to New Window, Previous/Next Tab.
/// `app.new` (File ▸ New Document) is registered at the `Application` level
/// (see `app.rs`) as the single `app.new` call site.
pub(crate) fn register_tab_actions(window: &ApplicationWindow) {
    let close_tab_action = SimpleAction::new("close-tab", None);
    close_tab_action.connect_activate(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_, _| {
            close_active_tab(&w);
        }
    ));
    window.add_action(&close_tab_action);

    let new_window_action = SimpleAction::new("new-window", None);
    new_window_action.connect_activate(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_, _| {
            let Some(app) = w.application() else { return };
            // Inherit this (the active) window's zoom and chrome (the `winstate`
            // state-scope rule) rather than resetting to
            // 100%/all-shown — see `new_window_from_source`.
            new_window_from_source(&app, "Scribobulate", crate::app::WELCOME, None, Some(&w));
        }
    ));
    window.add_action(&new_window_action);

    let move_tab_action = SimpleAction::new("move-tab-new-window", None);
    move_tab_action.connect_activate(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_, _| {
            move_tab_to_new_window(&w);
        }
    ));
    window.add_action(&move_tab_action);

    let next_tab_action = SimpleAction::new("next-tab", None);
    next_tab_action.connect_activate(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_, _| {
            cycle_tab(&w, 1);
        }
    ));
    window.add_action(&next_tab_action);

    let previous_tab_action = SimpleAction::new("previous-tab", None);
    previous_tab_action.connect_activate(glib::clone!(
        #[weak(rename_to = w)]
        window,
        move |_, _| {
            cycle_tab(&w, -1);
        }
    ));
    window.add_action(&previous_tab_action);

    // View ▸ Documents fast-switch: one stateful RADIO action whose string target
    // is a tab id. Each Documents menu item is `win.select-tab::<id>`; GTK checks
    // whichever item's target matches the action's current state, so "which tab is
    // active" is modelled as ACTION STATE — a plain tab switch then mutates NOTHING
    // (no menu rebuild), exactly the cheap path the researcher recommended
    // (GTK4Rs/AP-76). Like the `win.view-mode` radio, the real work lives in
    // `change-state` (NOT `activate`: GSimpleAction routes a stateful action's
    // activate straight to change-state), and `on_active_tab_changed` resyncs the
    // check via `set_action_state` — which uses `set_state`, so it never re-enters
    // this handler.
    // View ▸ Documents is a NESTED submenu, so its `change-state` must route the
    // GTK4Rs/AP-108 stray-popover dismissal (and `set_state`) — both carried by the
    // `nested_submenu_stateful_action` choke point rather than hand-wired here, so
    // this action cannot silently re-trip the seam the way an opt-in dismiss call
    // would (GTK4Rs/AP-108).
    let select_tab_action = nested_submenu_stateful_action(
        window,
        "select-tab",
        Some(glib::VariantTy::STRING),
        &"".to_variant(),
        move |w, value| {
            let Some(id) = value
                .get::<String>()
                .and_then(|s| s.parse::<winstate::TabId>().ok())
            else {
                return;
            };
            // `focus_page` fires switch-page → `on_active_tab_changed`, which does
            // the full active-tab retarget AND resyncs this action's state. No tab
            // list is rebuilt here, so there is no mid-activation GMenu mutation.
            if let (Some(chrome), Some(tab)) = (winstate::chrome(w), winstate::tab_by_id(id)) {
                chrome.tabs.focus_page(&tab.content_box);
            }
        },
    );
    window.add_action(&select_tab_action);
}

/// Move the active tab to the next (`delta = 1`) or previous (`delta = -1`)
/// page, wrapping around. A no-op with fewer than two tabs.
fn cycle_tab(window: &ApplicationWindow, delta: i32) {
    let Some(chrome) = winstate::chrome(window) else {
        return;
    };
    let n = chrome.tabs.n_pages();
    if n < 2 {
        return;
    }
    let Some(cur) = chrome.tabs.current_page() else {
        return;
    };
    let next = (cur as i32 + delta).rem_euclid(n as i32) as u32;
    chrome.tabs.set_current_page(Some(next));
}
