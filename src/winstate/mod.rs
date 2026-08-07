//! Per-window/per-tab state, held in a typed registry instead of untyped GLib qdata.
//!
//! State is split in two, keyed by scope (see the state-scope table below):
//!
//! - [`TabState`] — everything that is *per document*: path, source text, saved
//!   baseline (the content-gated save guard's comparison point — no on-disk
//!   mtime is tracked), the editor view/buffer, the swappable content slot,
//!   and the rest of the per-document bookkeeping. Keyed by a stable **tab id**
//!   (an incrementing counter, not a widget/window pointer), so a tab's identity
//!   survives a cross-window reparent (Phase 5's [`rehome_tab`]) where the
//!   owning window changes but the tab does not.
//! - [`WindowChrome`] — the widgets that are genuinely window-level furniture,
//!   shared across every tab in a window: the outline sidebar, the conflict/reload
//!   toasts, the footer status stack, the find-bar's shared widgets, the toolbar's
//!   shared Insert/Edit-labelled buttons, and the window-scoped zoom state.
//!
//! Phase 2 wraps each window's content in a `GtkNotebook` (`WindowChrome.notebook`)
//! and wires its `switch-page` signal to re-drive the single-source-of-truth
//! functions and to update which tab id is "active" ([`set_active_tab`]). To
//! keep the diff at every existing call site minimal, [`TabState`] keeps a
//! back-reference to its window's [`WindowChrome`] (`st.chrome()`), so a call
//! site that used to read `st.some_chrome_field` now reads
//! `st.chrome().some_chrome_field` — no call site needs a second registry
//! lookup. [`state`]'s signature and the "look up by `&ApplicationWindow`"
//! contract are unchanged.
//!
//! Phase 3 adds real multi-tab commands (File ▸ New Document / Close Tab, View ▸
//! New Window / Move Tab to New Window / Previous·Next Tab) via [`alloc_tab_id`],
//! [`add_tab`], and [`remove_tab`]. Moving a tab to a different window needs its
//! `chrome` back-reference to actually change (the tab itself — editor, buffer,
//! undo stack — is reused, not rebuilt, per the Phase 0 spike's whole point), so
//! unlike Phase 1/2, `TabState`'s chrome reference is a `RefCell<Rc<WindowChrome>>`
//! (`chrome_cell`, repointed via [`TabState::set_chrome`]) rather than a plain `Rc`.
//!
//! Lifetime: the registry holds the only owning `Rc<TabState>` (in `TABS`) and the
//! only owning `Rc<WindowChrome>` (in `WINDOWS`, plus the clone every one of its
//! window's tabs holds). Callbacks capture a *weak* window reference and call
//! [`state`] at fire time, so they never keep a `TabState` (and its widgets) alive
//! past the window — every tab entry is dropped by [`unregister`] on `destroy`.
//! Pointer reuse is not a hazard: the GLib main loop is single-threaded and the
//! entry is removed before the window is freed.
//!
//! Borrow discipline: each mutable field is its own `RefCell`/`Cell`, and the
//! widget handles are GObjects (cheap reference-counted clones), so callers take
//! short borrows — clone a handle out and drop the borrow before doing GTK work,
//! avoiding re-entrant `RefCell` panics across nested GTK callbacks.
//!
//! ## File layout
//!
//! The module is split by concern so each stays small; the pure, GTK-free pieces
//! keep their unit tests beside them (every submodule is in the coverage gate).
//! The crate-facing API (`winstate::TabState`, `winstate::state`, … — all the
//! `pub(crate)` items) is re-exported here so call sites are unchanged.
//!
//! * [`viewmode`] — [`ViewMode`], [`FocusedPane`], and the [`copy_target`] decision.
//! * [`scrollsync`] — [`ScrollDriver`] + the [`ScrollSync`] coalescing state (GTK4Rs/AP-16).
//! * [`ids`] — the [`TabId`] and [`WindowId`] identity newtypes.
//! * [`fmtinsert`] — [`FmtInsertKind`] (the Insert↔Edit Link/Image kind).
//! * [`selfdelete`] — [`SelfDeleteGuard`], the atomic-save round-trip guard (GTK4Rs/AP-62).
//! * [`status`] — the footer [`StatusStack`].
//! * [`swap`] — [`SwapState`] plus the crash-recovery snapshot debounce/cap arithmetic.
//! * [`navhistory`] — [`NavHistory`], the per-window Back/Forward history (TDD §23).
//! * [`chrome`] — [`WindowChrome`], the window-level furniture struct.
//! * [`tab`] — [`TabState`] + [`TabInit`] and their `impl`s.
//! * [`registry`] — the thread-local window/tab registry and its lookup/mutation API.
//! * [`decisions`] — the pure, display-free decision cores (save/edit-enable,
//!   window-title/tab-label formulas, external-change action, …).
//!
//! ## State scope — classify before writing seeding code
//!
//! Every piece of UI state is exactly one of three scopes, and the scope decides
//! where it lives, what seeds it at window-build time, and what happens on a tab
//! move. The three rules are different and not interchangeable — classify the
//! state *before* writing its seeding code.
//!
//! | Scope | Lives in | Seeded at build from | On tab move |
//! |---|---|---|---|
//! | **App-wide** — one instance for the whole app, no per-window copy (e.g. the reading theme: one CSS provider) | `theme::active()` (live) + `Session` (persisted) | resolved globally | global |
//! | **Window-scoped** — belongs to one window; two windows may legitimately disagree (zoom: one per-window CSS class; toolbar / status-bar / outline / toolbar-section visibility) | [`WindowChrome`] (zoom) / the window's own `win.*` action states (chrome), persisted per window in `session::WindowSession` | the source/active window, via `window::inherit_from` | **adopted from the destination** |
//! | **Tab-scoped** — travels with the document (`show_unsafe_images`, `allow_outside_links`, view mode, split arrangement) | [`TabState`] | the tab's own value | **travels with the tab** |
//!
//! Chrome is window-scoped and its live source of truth is the window's own
//! `win.*` action states (each toggle handler writes them; nothing else owns
//! them, so they cannot go stale). `window::read_window_chrome` is the single
//! reader, called by *both* the inherit-a-new-window path and the persist path,
//! so what a window inherits and what the session records cannot drift (ScrAP-136).
//! Window-scoped state is **inherited** by a brand-new window from its source but
//! **adopted from the destination** on a move into an existing one: zoom must
//! actively adopt (two tabs under one per-window CSS class cannot render at
//! different zooms — that is the real work `window/tabs/dnd.rs::wire_tab_arrival`
//! does for zoom, GTK4Rs/AP-77); chrome adopts by *construction* (not tab-scoped, so an
//! arriving tab carries no chrome and the destination's toggles are left alone —
//! `wire_tab_arrival` needs no chrome code). Editing `wire_tab_arrival` for a
//! window-scoped seeding change is the signal you have taken a wrong turn.

mod busynotice;
mod chrome;
mod decisions;
mod docepoch;
mod fmtinsert;
mod ids;
mod infotoast;
mod navhistory;
mod registry;
mod scrollsync;
mod selfdelete;
mod status;
mod swap;
mod tab;
mod viewmode;
mod writegate;

pub(crate) use busynotice::BusyNotice;
pub(crate) use chrome::WindowChrome;
pub(crate) use decisions::{
    edit_actions_enabled, external_change_action, is_blank_welcome, line_col_indicator,
    save_enabled, save_is_safe, tab_label_markup, window_title_for_tabs, ExternalChange,
    TabBadgeState, APP_NAME,
};
pub(crate) use docepoch::DocEpoch;
pub(crate) use fmtinsert::FmtInsertKind;
pub(crate) use ids::{TabId, WindowId};
pub(crate) use infotoast::InfoToast;
pub(crate) use navhistory::{NavDir, NavHistory};
pub(crate) use registry::{
    add_tab, alloc_tab_id, chrome, nav_can, nav_record, nav_step, nav_suppress, register,
    rehome_tab, remove_tab, set_active_tab, state, tab_by_content_box, tab_by_id, tab_count,
    tab_for_descendant, tabs_for_window, unregister,
};
pub(crate) use scrollsync::{ScrollDriver, ScrollSync};
pub(crate) use selfdelete::SelfDeleteGuard;
pub(crate) use status::{StatusCtx, StatusStack, ERROR_NOTICE_TIME};
pub(crate) use swap::{next_delay_ms, SwapState, MAX_LATENCY_MS};
pub(crate) use tab::{TabInit, TabState};
pub(crate) use viewmode::{copy_target, FocusedPane, ViewMode};
pub(crate) use writegate::WriteGate;
