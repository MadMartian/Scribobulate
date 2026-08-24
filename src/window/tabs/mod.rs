//! Tab lifecycle subsystem: everything that
//! needs `winstate`/window-domain knowledge to create, switch, close, relocate,
//! and label a window's tabs (`widgets/tab` owns the widget itself).
//! Decomposed from the former monolithic `window/tabs.rs` into focused
//! submodules:
//!   - [`switch`]     — active-tab `switch-page` re-drive + caret-overlay retarget
//!   - [`lifecycle`]  — tab construction (editor/search/buffer signals) + create/close
//!   - [`actions`]    — `win.*` tab actions (close/new-window/move/next-prev/select)
//!   - [`dnd`]        — Move Tab + cross-window drag + drag-to-desktop
//!   - [`contextmenu`]— per-tab `×` button and right-click menu
//!   - [`documents`]  — window title, View ▸ Documents submenu, tab-strip labels
//!
//! Submodules reach each other's tab-internal helpers through the private globs
//! below; the `pub(crate) use` re-exports expose the subsystem's surface to the
//! rest of the `window` module (and, for the four `pub(crate)` entry points, to
//! `app.rs` via `crate::window::…`).

mod actions;
mod contextmenu;
mod dnd;
mod documents;
mod lifecycle;
mod switch;

// Intra-subsystem sibling resolution: each submodule's `use super::*` (super =
// this module) reaches its siblings' tab-internal `pub(super)` helpers — the
// ones NOT re-exported below — through these private globs. Only the three
// submodules that actually export such a helper need a glob here:
//   switch    → detach_overlay_from, resync_tab_action_state (the latter also
//               called explicitly from dnd.rs's tab-arrival handler — the
//               pop-out action-mirror fix — see that call site)
//   lifecycle → close_active_tab, close_specific_tab, confirm_close_tab,
//               close_other_tabs
//   dnd       → move_tab_to_new_window
// (actions/contextmenu/documents export nothing consumed by a sibling; their
// window-facing surface travels through the `pub(crate) use` re-exports below.)
use dnd::*;
use lifecycle::*;
use switch::*;

// The subsystem surface visible to the rest of `window` (brought into window
// scope by `window/mod.rs`'s `use tabs::*`). The four that `app.rs` calls as
// `crate::window::…` are additionally re-exported at window level there.
pub(crate) use actions::register_tab_actions;
pub(crate) use contextmenu::wire_tab_close_and_menu;
/// The tab context menu's item enumeration. Re-exported so the mnemonics guard can
/// DERIVE its check from the menu that ships rather than mirror it — see the enum's own
/// doc comment for what the mirror cost. Test-gated because the builder that walks it
/// lives in `contextmenu` itself, so the guard is the only consumer out here, and an
/// ungated re-export is an unused import the `-D warnings` gate rejects.
#[cfg(test)]
pub(crate) use contextmenu::TabMenuItem;
pub(crate) use dnd::{wire_tab_arrival, wire_tab_bar_dnd};
pub(crate) use documents::{
    badge_tab_label, refresh_active_tab_label, refresh_documents_button, refresh_documents_menu,
    update_window_title,
};
/// The editor builder itself, reached directly only by `farscroll`'s integration
/// tests so they exercise the editor the app actually ships. Carries their cfg,
/// not a bare `#[cfg(test)]`, per POLICY's test-helper rule.
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) use lifecycle::build_tab_editor;
pub(crate) use lifecycle::{
    add_new_document_tab, assemble_tab_core, create_tab_in_window, host_window, resolve_tab_window,
    window_of_content_box,
};
pub(crate) use switch::{
    retarget_format_overlay, start_deferred_prerender_pump, wire_tab_switch_page,
};
