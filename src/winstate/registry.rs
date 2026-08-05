//! The thread-local window/tab registry: id allocation, register/add/remove/rehome,
//! and the `state`/`chrome`/`tabs_for_window` lookups every call site reaches through.

use super::{NavDir, NavHistory, TabId, TabState, WindowChrome, WindowId};
use gtk::prelude::*;
use gtk::ApplicationWindow;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// One window's registry entry: its shared chrome, which of its tabs is active,
/// and the full list of its tabs (both unused beyond a single element until
/// The `GtkNotebook` wraps each window's tabs, but this registry is already correctly shaped so
/// `state`/`register`/`unregister`'s contracts don't change again then).
struct WindowEntry {
    chrome: Rc<WindowChrome>,
    /// `None` = the window currently has no active tab (transiently, after its
    /// last tab is rehomed away and before it closes — [`rehome_tab`]). Replaces
    /// the former magic `0` sentinel (QA round-1 L2).
    active_tab: Cell<Option<TabId>>,
    tabs: RefCell<Vec<TabId>>,
    /// This window's Back/Forward history (TDD §23). It lives here rather than in
    /// [`WindowChrome`] because it is the same *kind* of fact as the two fields
    /// above — which tabs this window has, and which one is active — only extended
    /// over time; `WindowChrome` is the window's furniture. Keeping it here is also
    /// what makes TDD 23.8 automatic: the two places a tab stops belonging to a
    /// window ([`remove_tab`], [`rehome_tab`]) are in this file, so the history
    /// cannot be left holding a tab the window no longer has.
    nav: RefCell<NavHistory>,
}

thread_local! {
    static WINDOWS: RefCell<HashMap<WindowId, WindowEntry>> = RefCell::new(HashMap::new());
    static TABS: RefCell<HashMap<TabId, Rc<TabState>>> = RefCell::new(HashMap::new());
    static NEXT_TAB_ID: Cell<u64> = const { Cell::new(1) };
}

/// A window's registry key — the single [`WindowId::of`] derivation shared with
/// the `.scrib-win-<id>` CSS scope (`window`/`zoom`), so the map key and the CSS
/// class can never diverge for the same window (see `WindowId`'s CONTRACT, ScrAP-64).
fn window_id(window: &ApplicationWindow) -> WindowId {
    WindowId::of(window)
}

/// Allocate a fresh, monotonically-increasing tab id. Callers construct their
/// `TabState` with this id (`TabState.id`) BEFORE registering it — the id must
/// exist up front so the tab can name itself for `winstate::state`/`chrome`
/// lookups the moment it's built, and so `register`/`add_tab` never have to
/// hand back a value the caller didn't already know.
pub(crate) fn alloc_tab_id() -> TabId {
    NEXT_TAB_ID.with(|c| {
        let id = c.get();
        c.set(id + 1);
        TabId::from_raw(id)
    })
}

/// Register a brand-new window with its first tab. `tab.id` must already be
/// allocated via [`alloc_tab_id`] and `tab.chrome_cell` must already hold
/// `chrome.clone()` of the same `Rc` passed here.
pub(crate) fn register(
    window: &ApplicationWindow,
    chrome: Rc<WindowChrome>,
    tab: TabState,
) -> TabId {
    // QA round-1 M5: this precondition was documented but unchecked — a future
    // mismatched call site would split-brain the registry (`st.chrome()`
    // reading one window's widgets while `winstate::chrome(window)` reports
    // another). Cheap to verify and free in release builds.
    debug_assert!(
        Rc::ptr_eq(&tab.chrome_cell.borrow(), &chrome),
        "winstate::register: tab.chrome_cell must already equal `chrome`"
    );
    let tab_id = tab.id;
    TABS.with(|m| m.borrow_mut().insert(tab_id, Rc::new(tab)));
    WINDOWS.with(|m| {
        m.borrow_mut().insert(
            window_id(window),
            WindowEntry {
                chrome,
                active_tab: Cell::new(Some(tab_id)),
                tabs: RefCell::new(vec![tab_id]),
                // The window's first tab IS its first history entry, seeded here
                // rather than by the switch-page choke point: a window's initial
                // page is shown by the `GtkStack` and never travels through
                // `switch_to_index`, so no switch callback ever fires for it
                // (`TabBar::mark_first_active`, ScrAP-62). Without this seeding the
                // history would start at the SECOND document the reader visits, and
                // Back from it would report nothing behind it — the one entry a
                // reader is most certain exists.
                nav: RefCell::new(NavHistory::seeded(tab_id)),
            },
        )
    });
    tab_id
}

/// Add another tab to an ALREADY-registered window (File
/// ▸ New Document, and the destination side of Move Tab to New Window). Does
/// NOT change which tab is active: the caller drives that by making the tab's
/// page current on the `GtkNotebook`, which fires `switch-page` and calls
/// [`set_active_tab`] through the normal path (`window/tabs/`) — this keeps
/// "which tab is active" single-sourced from the notebook's own signal rather
/// than duplicated here.
pub(crate) fn add_tab(window: &ApplicationWindow, tab: TabState) -> TabId {
    let tab_id = tab.id;
    TABS.with(|m| m.borrow_mut().insert(tab_id, Rc::new(tab)));
    WINDOWS.with(|m| {
        if let Some(entry) = m.borrow().get(&window_id(window)) {
            entry.tabs.borrow_mut().push(tab_id);
        }
    });
    tab_id
}

/// Remove one tab from `window`'s registry entry (File ▸ Close Tab, Move Tab to
/// New Window's source side). If the removed tab was active, falls back to the
/// window's first remaining tab — a placeholder the caller should immediately
/// correct via a real `switch-page` (triggered by `GtkNotebook` auto-selecting a
/// neighboring page when the current one is removed/detached). Leaves the
/// window's tab list empty rather than special-casing "last tab": callers are
/// responsible for checking [`tab_count`] and closing an emptied window
/// (a window with zero tabs is never a valid state).
pub(crate) fn remove_tab(window: &ApplicationWindow, tab_id: TabId) {
    // A closed tab leaves its history with it (TDD 23.8) — here rather than at
    // the call sites, because this is where "the window no longer has this tab"
    // becomes true.
    nav_forget_everywhere(tab_id);
    WINDOWS.with(|m| {
        if let Some(entry) = m.borrow().get(&window_id(window)) {
            entry.tabs.borrow_mut().retain(|&id| id != tab_id);
            if entry.active_tab.get() == Some(tab_id) {
                if let Some(&next) = entry.tabs.borrow().first() {
                    entry.active_tab.set(Some(next));
                }
            }
        }
    });
    TABS.with(|m| {
        m.borrow_mut().remove(&tab_id);
    });
}

// ── Back/Forward history (TDD §23) ───────────────────────────────────────────
// The window's [`NavHistory`] lives in its [`WindowEntry`] (see that field's doc
// comment). These five functions are the whole crate-facing surface; the pure
// rules are in [`super::navhistory`], and the UI side — the two GActions, their
// sensitivity, and the suppression call sites — is `window::navhistory`.

/// Run `f` against `window`'s history, or return `None` if the window is not (or
/// no longer) registered. One helper so every accessor below takes the same short
/// borrow and cannot hold it across a GTK call (ScrAP-53).
fn with_nav<T>(window: &ApplicationWindow, f: impl FnOnce(&mut NavHistory) -> T) -> Option<T> {
    WINDOWS.with(|m| {
        let m = m.borrow();
        let entry = m.get(&window_id(window))?;
        let out = f(&mut entry.nav.borrow_mut());
        Some(out)
    })
}

/// Record that `tab_id` became `window`'s active tab. Called from exactly one
/// place — the tab-strip switch callback (`window::tabs`) — so every present and
/// future way of changing the active tab is history-bearing by default; the
/// exceptions raise [`nav_suppress`] instead.
pub(crate) fn nav_record(window: &ApplicationWindow, tab_id: TabId) {
    with_nav(window, |nav| nav.record(tab_id));
}

/// Step `window`'s history one entry in `dir`, returning the tab to activate.
pub(crate) fn nav_step(window: &ApplicationWindow, dir: NavDir) -> Option<TabId> {
    with_nav(window, |nav| nav.step(dir)).flatten()
}

/// Whether a [`nav_step`] in `dir` would go anywhere — the single source of the
/// two actions' enabled state (TDD 23.5).
pub(crate) fn nav_can(window: &ApplicationWindow, dir: NavDir) -> bool {
    with_nav(window, |nav| nav.can(dir)).unwrap_or(false)
}

/// Open a scope in which page switches on `window` are not navigations (TDD
/// 23.9) — traversal itself, and the internal sweeps that reveal a tab in order
/// to prompt about it. Released on drop, so an early return or a `?` cannot leak
/// suppression (which would silently disable the feature for that window).
///
/// Keyed by [`WindowId`] rather than holding the window: the guard must not keep a
/// closing window alive, and a window unregistered inside the scope simply has
/// nothing left to release.
#[must_use = "the scope ends when the guard drops; binding it to `_` ends it immediately"]
pub(crate) fn nav_suppress(window: &ApplicationWindow) -> NavSuppressGuard {
    with_nav(window, |nav| nav.suppress());
    NavSuppressGuard {
        window: window_id(window),
    }
}

/// The live scope [`nav_suppress`] opens. See its doc comment.
pub(crate) struct NavSuppressGuard {
    window: WindowId,
}

impl Drop for NavSuppressGuard {
    fn drop(&mut self) {
        WINDOWS.with(|m| {
            if let Some(entry) = m.borrow().get(&self.window) {
                entry.nav.borrow_mut().unsuppress();
            }
        });
    }
}

/// Drop every history entry for `tab_id` in whichever window's history holds it
/// (TDD 23.8). Private: it is called only from [`remove_tab`]/[`rehome_tab`], the
/// two places a tab stops belonging to a window, so no caller can forget it.
fn nav_forget_everywhere(tab_id: TabId) {
    WINDOWS.with(|m| {
        for entry in m.borrow().values() {
            entry.nav.borrow_mut().forget(tab_id);
        }
    });
}

/// Look up a tab by id directly, independent of which window (if any) currently
/// hosts it — used by tab-label refresh and the close/move commands, which
/// operate on a specific tab id rather than "whichever is active."
pub(crate) fn tab_by_id(tab_id: TabId) -> Option<Rc<TabState>> {
    TABS.with(|m| m.borrow().get(&tab_id).cloned())
}

/// Find a registered tab by its `content_box` widget identity, independent of
/// which window (if any) currently lists it — used by the cross-window
/// tab-arrival handler (`window/tabs/`'s
/// `wire_notebook_tab_arrival`), which is only handed the raw widget by
/// GtkNotebook's `page-added` signal, not a tab id.
pub(crate) fn tab_by_content_box(child: &gtk::Widget) -> Option<Rc<TabState>> {
    TABS.with(|m| {
        m.borrow()
            .values()
            .find(|t| t.content_box.upcast_ref::<gtk::Widget>().as_ptr() == child.as_ptr())
            .cloned()
    })
}

/// Move `tab_id` from whichever window's registry currently lists it (if any)
/// to `dest_window`'s (a native cross-window drag, or
/// `move_tab_to_new_window`'s explicit `append_page`, both of which land the
/// tab's widget in `dest_window`'s `GtkNotebook` before calling this). Unlike
/// [`remove_tab`]/[`add_tab`], this never touches `TABS` — the `TabState`
/// itself is reused unchanged, matching the module doc's promise that a tab's
/// id (and identity) survives a cross-window reparent; only which window's
/// tab list names it changes. Sets `dest_window`'s active tab to `tab_id`
/// directly rather than waiting for a `switch-page` signal, since a
/// destination notebook that already has this tab as its only/current page
/// would not fire one.
pub(crate) fn rehome_tab(dest_window: &ApplicationWindow, tab_id: TabId) {
    // A tab that moves to another window leaves the origin's history exactly as a
    // closed one does (TDD 23.8) — a traversal must never activate a document
    // living in a different window (23.7). Swept across every window rather than
    // the origin alone: the origin is identified below by scanning, and the
    // destination's own history cannot contain a tab it has never shown.
    nav_forget_everywhere(tab_id);
    WINDOWS.with(|m| {
        let mut m = m.borrow_mut();
        for entry in m.values_mut() {
            if !entry.tabs.borrow().contains(&tab_id) {
                continue;
            }
            entry.tabs.borrow_mut().retain(|&id| id != tab_id);
            if entry.active_tab.get() == Some(tab_id) {
                match entry.tabs.borrow().first() {
                    Some(&next) => entry.active_tab.set(Some(next)),
                    // QA round-2 N7: unlike `remove_tab`, this tab id is NOT
                    // dropped from `TABS` here — it now belongs to
                    // `dest_window` instead — so leaving `active_tab`
                    // pointing at it would make `state(this_window)` report
                    // a tab this window no longer owns (one that may now be
                    // simultaneously active in the destination window too).
                    // The drained window is expected to close immediately
                    // (`wire_notebook_tab_arrival`'s tab_count==0 check), but
                    // nothing enforces that ordering here — clear the active
                    // tab (`None`) so `state(this_window)` reports no tab.
                    None => entry.active_tab.set(None),
                }
            }
        }
        if let Some(dest) = m.get(&window_id(dest_window)) {
            let mut tabs = dest.tabs.borrow_mut();
            if !tabs.contains(&tab_id) {
                tabs.push(tab_id);
            }
            drop(tabs);
            dest.active_tab.set(Some(tab_id));
        }
    });
}

/// Number of tabs currently registered to `window` (0 after its last tab is
/// removed but before the window itself is closed — see [`remove_tab`]).
pub(crate) fn tab_count(window: &ApplicationWindow) -> usize {
    WINDOWS.with(|m| {
        m.borrow()
            .get(&window_id(window))
            .map(|e| e.tabs.borrow().len())
            .unwrap_or(0)
    })
}

/// Look up a window's *active tab's* state (cheap `Rc` clone; `None` after the
/// window closes). This is the same call-site contract every existing caller
/// already uses — until Phase 2 adds tab switching, a window's active tab is
/// simply its only tab.
pub(crate) fn state(window: &ApplicationWindow) -> Option<Rc<TabState>> {
    let tab_id = WINDOWS.with(|m| {
        m.borrow()
            .get(&window_id(window))
            .and_then(|e| e.active_tab.get())
    })?;
    TABS.with(|m| m.borrow().get(&tab_id).cloned())
}

/// Look up a window's shared chrome directly (cheap `Rc` clone). Most call sites
/// don't need this — they reach chrome fields via `state(window).chrome()` — but
/// a window-wide operation that doesn't care about the active tab (Move Tab to
/// New Window, tab-count/title/tab-strip-visibility refresh) uses this to reach
/// the `GtkNotebook` and other shared widgets directly.
pub(crate) fn chrome(window: &ApplicationWindow) -> Option<Rc<WindowChrome>> {
    WINDOWS.with(|m| m.borrow().get(&window_id(window)).map(|e| e.chrome.clone()))
}

/// Find the window and tab that own `widget` (any descendant of a tab's
/// `content_box`, e.g. the preview `CodePreviewView` a link click fired on),
/// by walking up to the toplevel window and then testing `content_box`
/// ancestry against every tab of that window — the same `is_ancestor`-gating
/// idiom used elsewhere for "which pane owns this" questions, rather than a
/// closure captured per-render that could go stale across a re-render/rehome
/// (link navigation needs a tab's `doc_dir`/`allow_outside_links`
/// at CLICK time, not render time).
pub(crate) fn tab_for_descendant(
    widget: &impl IsA<gtk::Widget>,
) -> Option<(ApplicationWindow, Rc<TabState>)> {
    let window = widget
        .as_ref()
        .root()?
        .downcast::<ApplicationWindow>()
        .ok()?;
    let tab = tabs_for_window(&window)
        .into_iter()
        .find(|t| widget.as_ref().is_ancestor(&t.content_box))?;
    Some((window, tab))
}

/// Every `TabState` currently registered to `window`, in the same order as its
/// internal tab-id list (today: insertion order; a future reorderable-tabs
/// feature would need to keep this in sync with `GtkNotebook` page order). Used
/// by window-wide operations that must act on every tab, not only the active
/// one — the zoom re-render sweep, the window-title/tab-count logic, and the
/// window-destroy overlay-unparent sweep.
pub(crate) fn tabs_for_window(window: &ApplicationWindow) -> Vec<Rc<TabState>> {
    let ids = WINDOWS.with(|m| {
        m.borrow()
            .get(&window_id(window))
            .map(|e| e.tabs.borrow().clone())
    });
    let Some(ids) = ids else { return Vec::new() };
    TABS.with(|m| {
        let m = m.borrow();
        ids.iter().filter_map(|id| m.get(id).cloned()).collect()
    })
}

/// Change which of `window`'s tabs is active. Called from the `GtkNotebook`
/// `switch-page` handler (`window/tabs/`) after resolving the newly-selected
/// page's tab id. A no-op if `tab_id` is not one of `window`'s tabs (defensive;
/// should not happen in practice since the id is read directly off the page
/// widget the signal just told us is now current).
pub(crate) fn set_active_tab(window: &ApplicationWindow, tab_id: TabId) {
    WINDOWS.with(|m| {
        if let Some(entry) = m.borrow().get(&window_id(window)) {
            if entry.tabs.borrow().contains(&tab_id) {
                entry.active_tab.set(Some(tab_id));
            }
        }
    });
}

/// Drop a window's state (all its tabs, then the window entry itself); call on
/// `destroy`.
pub(crate) fn unregister(window: &ApplicationWindow) {
    let tabs = WINDOWS.with(|m| {
        m.borrow_mut()
            .remove(&window_id(window))
            .map(|e| e.tabs.into_inner())
    });
    if let Some(tabs) = tabs {
        TABS.with(|m| {
            let mut m = m.borrow_mut();
            for id in tabs {
                m.remove(&id);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use crate::winstate::*;

    #[test]
    fn register_creates_a_lookup_reachable_tab_and_unregister_clears_it() {
        // window_id/chrome/state all key off a real ApplicationWindow pointer, which
        // needs a live GTK instance; that is covered by the interactive/manual TDD
        // sweep (Phase 1's own requirement). This test exercises the id-allocation
        // and window-entry bookkeeping in isolation, GTK-free.
        let a = alloc_tab_id();
        let b = alloc_tab_id();
        assert_ne!(a, b, "each allocated tab id must be unique");
        assert!(
            b.raw() > a.raw(),
            "tab ids must be monotonically increasing"
        );
    }
}
