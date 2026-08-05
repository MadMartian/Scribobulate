//! Cross-window tab relocation (Phase 0 spike): the bare-window spawner both
//! the explicit "Move Tab to New Window" action and the drag-to-desktop case
//! share, the single arrival callback that rehomes any tab entering a window's
//! strip from another window, and the one `GtkDragSource`/`GtkDropTarget` pair
//! that drives in-strip reorder AND cross-window move AND drag-to-desktop.
//! Split out of the former monolithic `window/tabs.rs`.

use super::super::*;
use super::*;
use crate::widgets::tab::TabView;

/// Build a brand-new window meant to host exactly one already-existing tab —
/// Move Tab to New Window's destination, and the X11 desktop-drop
/// enhancement below — then immediately drop its throwaway blank starter tab
/// so the caller's own tab becomes the window's only content. Reuses
/// `new_window`'s full chrome/action/signal construction rather than
/// duplicating that ~200-line setup for these two call sites.
///
/// Transiently returns a window with zero tabs registered — normally an
/// invalid state (a window with zero tabs) — but safe here because both callers
/// install the caller's real tab into this window's tab strip immediately
/// afterward, within the same synchronous call, before anything reaches the
/// main loop; nothing is ever painted or observed in between.
///
/// `source` is the window the tab is popping OUT of (the `winstate`
/// state-scope rule): a pop-out has nothing else to constrain its
/// window-scoped state, so it inherits the source's zoom and chrome rather than
/// resetting to the 100%/all-shown defaults — see `new_window_from_source`'s
/// doc comment for the full rationale.
fn spawn_bare_window_for_tab(
    app: &Application,
    source: &ApplicationWindow,
) -> Option<(ApplicationWindow, Rc<winstate::WindowChrome>)> {
    let win = new_window_from_source(app, "Scribobulate", crate::app::WELCOME, None, Some(source));
    // QA round-1 L6: `new_window` always registers its chrome before
    // returning, so this should never be `None` — but this function is
    // reachable from the `drag-cancel` dispatch frame (via
    // `spawn_window_hosting_tab`), a non-unwindable GTK/GDK callback where a
    // panic (an `.expect()` here used to be one) becomes a hard process
    // abort, not a catchable Rust panic. An early return degrades to "the
    // drag silently does nothing" instead — safe, if this invariant is ever
    // violated by a future change elsewhere.
    let Some(chrome) = winstate::chrome(&win) else {
        log::error!("spawn_bare_window_for_tab: new_window returned an unregistered window");
        return None;
    };
    if let Some(starter) = state(&win) {
        let starter_id = starter.id;
        // QA round-1 H1 (ScrAP-61): the window's single caret overlay was parented
        // to this throwaway starter tab's editor at window creation. Detach it before
        // the tab's last strong reference drops, or the editor finalizes while it
        // still has the popover child (`Finalizing … still has children left`
        // critical) and the popover subtree leaks with a dangling parent pointer.
        // The overlay lives on in WindowChrome; the caller installs its real tab
        // immediately after, whose first activation re-parents the overlay
        // (`retarget_format_overlay`).
        detach_overlay_from(&chrome, &starter.editor);
        if let Some(idx) = chrome.tabs.page_num(&starter.content_box) {
            chrome.tabs.remove_page(Some(idx));
        }
        winstate::remove_tab(&win, starter_id);
    }
    Some((win, chrome))
}

/// Spawn a new window and immediately install `content_box` as its one tab
/// (QA round-1 M3): folds the append into the spawn so the zero-tab
/// intermediate state `spawn_bare_window_for_tab` returns is never observable
/// outside this function, and both call sites collapse from the previously
/// duplicated detach→spawn→append sequence to detach→spawn-and-install. The
/// module comment's claim that the explicit action and the drag-detach
/// "share one tail" now actually holds for this half of the sequence too,
/// not just `wire_tab_arrival`.
///
/// QA round-2 R2-1: spawns the destination window BEFORE detaching
/// `content_box` from `source_tabs` — not after. `spawn_bare_window_for_tab`
/// is fallible (QA round-1 L6's `Option` return, for the non-unwindable
/// `drag-cancel` dispatch path), and detaching first would leave
/// `content_box` orphaned (removed from the source strip, appended
/// nowhere) on that near-impossible failure path — trading L6's process
/// abort for a silent, harder-to-notice data-loss bug. Spawning first means
/// a `None` here leaves `content_box` exactly where it started: still
/// attached to `source_tabs`, nothing detached, nothing lost.
fn spawn_window_hosting_tab(
    app: &Application,
    source: &ApplicationWindow,
    source_tabs: &TabView,
    content_box: &gtk::Box,
) -> Option<ApplicationWindow> {
    let (win, chrome) = spawn_bare_window_for_tab(app, source)?;

    // ScrAP-90's shape, third call site — and it lives HERE, in the shared tail, not
    // in `move_tab_to_new_window` above, because BOTH pop-out paths funnel through this
    // function and only one of them is the menu action. The source window's single
    // format overlay is `set_parent`-attached to its active tab's editor (one per
    // window — ScrAP-61), and that is the editor being handed away. GTK does not
    // auto-unparent `set_parent`ed popovers, so the parent's teardown path must, and
    // the destination window immediately parents its OWN overlay onto the same editor
    // via `retarget_format_overlay`.
    //
    // Which path reaches the damage matters, and is not the obvious one. The MENU
    // action is disabled for a window's only tab (TDD 15.4), so it always leaves a
    // surviving tab behind; that tab becomes active, `retarget_format_overlay` fires,
    // and its popdown→unparent→set_parent pulls the overlay off the moved editor as a
    // SIDE EFFECT — the stale parent never forms. The DRAG-to-desktop path has no such
    // restriction (TDD 7.7 keeps it reachable for a single-tab window), so it can strip
    // a window of its last tab with nothing left to trigger that incidental rescue.
    // Putting the call in the action would therefore have guarded the path that already
    // self-heals and missed the one that does not.
    //
    // Deliberately resolved from `content_box` rather than taken as a parameter: a
    // future third caller cannot forget to pass it. Internally guarded, so it is a
    // no-op unless THIS editor hosts the overlay.
    if let Some(tab) = winstate::tab_by_content_box(content_box.upcast_ref::<gtk::Widget>()) {
        detach_overlay_from(&tab.chrome(), &tab.editor);
    }

    // Landing on the neighbour because a tab was handed away is not a navigation
    // (TDD 23.8/23.9) — the same rule as closing one; `detach_tab` makes the
    // source strip switch synchronously. The moved tab's own entries are dropped
    // by `winstate::rehome_tab`, which `wire_tab_arrival` runs on the append below.
    {
        let _no_history = winstate::nav_suppress(source);
        source_tabs.detach_tab(content_box);
    }
    chrome.tabs.append_page(content_box);
    Some(win)
}

/// View ▸ Move Tab to New Window: detach the active tab (preserving its
/// editor/buffer/undo stack intact — `detach_tab`, never `remove_page`, per
/// the Phase 0 spike's ScrAP-43 findings) and hand it to a brand-new window's
/// tab strip. Does not itself repoint the `winstate` registry, the tab's
/// chrome back-reference, or either window's title/tab strip:
/// `wire_tab_arrival` below — wired on every window's tab strip, this new
/// one included — does that entire rehome the moment `append_page` fires its
/// tab-arrived callback, exactly as it would for a native cross-window drag.
/// This is the Phase 0 spike's design intent: the explicit action and a
/// drag-detach share one tail, not two parallel implementations.
///
/// Also the target of the per-tab context menu's "Move to New Window" entry
/// (N2) for a SPECIFIC, possibly-inactive tab: the menu handler switches to
/// that tab first (a cheap, harmless `set_current_page`), then calls this —
/// reusing the active-tab path rather than a parallel specific-tab one.
pub(super) fn move_tab_to_new_window(window: &ApplicationWindow) {
    let Some(app) = window.application() else {
        return;
    };
    let Some(source_chrome) = winstate::chrome(window) else {
        return;
    };
    let Some(tab) = state(window) else { return };
    let content_box = tab.content_box.clone();

    // `spawn_window_hosting_tab` already logs on the near-impossible `None`
    // path (QA round-1 L6) and leaves `content_box` untouched on it (QA
    // round-2 R2-1); nothing further to do here on that outcome.
    let _ = spawn_window_hosting_tab(&app, window, &source_chrome.tabs, &content_box);
}

/// Reacts whenever a tab that belongs to a DIFFERENT window's `winstate`
/// registry arrives in `window`'s tab strip. Every arrival funneling through
/// this one callback is one of exactly three explicit, app-controlled
/// `detach_tab`→`append_page` pairs: `move_tab_to_new_window`, the
/// cross-window drag drop (`wire_tab_bar_dnd`'s `connect_drop`), and the
/// drag-to-desktop new-window case (`wire_tab_bar_dnd`'s `connect_drag_cancel`)
/// — rather than three separate rehome implementations. A tab already
/// correctly registered to `window` (an ordinary `create_tab_in_window` tab,
/// or the very first page built before any tab is even registered) is a
/// no-op: its `chrome_cell` already points at `window`'s own chrome, so the
/// identity check below returns immediately.
pub(crate) fn wire_tab_arrival(window: &ApplicationWindow, tab_view: &TabView) {
    tab_view.connect_page_added(glib::clone!(
        #[weak(rename_to = dest)]
        window,
        move |_tv, child, _page_num| {
            let Some(dest_chrome) = winstate::chrome(&dest) else {
                return;
            };
            let Some(tab) = winstate::tab_by_content_box(child) else {
                return;
            };
            let old_chrome = tab.chrome();
            if Rc::ptr_eq(&old_chrome, &dest_chrome) {
                return;
            }
            // Identify the source window by matching chrome identity, not a
            // stored back-reference — TabState/WindowChrome deliberately hold no
            // ApplicationWindow handle (winstate.rs module doc: avoids a
            // reference cycle that would keep a closed window's chrome alive).
            let source_window = dest.application().and_then(|app| {
                app.windows().into_iter().find_map(|w| {
                    let w = w.downcast::<ApplicationWindow>().ok()?;
                    let c = winstate::chrome(&w)?;
                    Rc::ptr_eq(&c, &old_chrome).then_some(w)
                })
            });

            // The winstate registry rehome is pure-Rust (thread_local maps) and
            // safe to do NOW — keeping it consistent this instant means no handler
            // that fires during the drop can observe the tab registered to neither
            // window. Everything BELOW that touches the tab strip's page list or
            // the strip widget itself is deferred (see the big comment on the idle).
            tab.set_chrome(dest_chrome.clone());
            winstate::rehome_tab(&dest, tab.id);

            // Defer every tab-strip mutation out of THIS dispatch anyway: the
            // arrival callback still fires synchronously inside `append_page`/
            // `detach_tab`'s own call chain, and mutating either strip's state
            // immediately risks reading it mid-update. By the next main-loop turn
            // both strips have settled and the triggering call has returned.
            // Capture only a strong ref to the child WIDGET, and re-derive
            // page_num inside the idle.
            // Zoom is window-scoped (WindowChrome.zoom_level): a tab moved between
            // windows at DIFFERENT zoom levels adopts the destination's font for free
            // (the shared per-window `.scrib-win-<id>` CSS class, ScrAP-64) but its
            // per-render PIXEL geometry — heading scale, code-block padding, list
            // indents, and above all the anchored table-cell layout — was baked at the
            // SOURCE zoom and does NOT move on its own. Left unsynced, the two halves
            // desync into a garbled preview (destination-sized text over source-sized
            // geometry — the cross-window face of ScrAP-64's symptom 2, table cells
            // overlapping). So re-render the arriving tab's preview at the destination
            // zoom. Captured before the idle; the comparison skips the (common)
            // same-zoom move, and `rerender_tab_preview_in_place` no-ops in edit mode.
            let source_zoom = old_chrome.zoom_level.get();
            let child = child.clone();
            let dest_weak = dest.downgrade();
            let src_weak = source_window.map(|s| s.downgrade());
            glib::idle_add_local_once(move || {
                if let Some(dest) = dest_weak.upgrade() {
                    if let Some(dc) = winstate::chrome(&dest) {
                        dc.tabs.focus_page(&child);
                        update_window_title(&dest);
                        // Explicit belt-and-suspenders resync: a
                        // per-tab toggle's `win.*` GAction is seeded `false` at
                        // every new window's construction, independent of
                        // whichever tab lands there — `focus_page` above SHOULD
                        // already catch this up via `switch-page` →
                        // `on_active_tab_changed` → `resync_tab_action_state`
                        // (verified: `TabView::switch_to_index`'s no-op guard
                        // compares against the PRIOR `active_idx`, which
                        // `spawn_bare_window_for_tab` leaves `None` after removing
                        // the throwaway starter tab, so the arriving tab's index
                        // is never a same-index no-op here — confirmed by a
                        // mutation test: removing this explicit call does not
                        // currently fail `move_tab_to_new_window_mirrors_the_tabs_
                        // own_toggle_state` below). Kept anyway as an explicit,
                        // enforced choke point (gtk4-rs skill's "make the pitfall
                        // surface at write-time" principle) rather than relying on
                        // an implicit index-bookkeeping invariant elsewhere in
                        // `widgets/tab` that a future refactor could silently
                        // break without anything here noticing — the cost of
                        // calling it is one cheap `set_state` per arrival.
                        if let Some(tab) = winstate::tab_by_content_box(&child) {
                            resync_tab_action_state(&dest, &tab);
                        }
                        let dest_zoom = dc.zoom_level.get();
                        if (dest_zoom - source_zoom).abs() > f64::EPSILON {
                            if let Some(tab) = winstate::tab_by_content_box(&child) {
                                rerender_tab_preview_in_place(
                                    &tab,
                                    tab.view_mode.get(),
                                    dest_zoom,
                                    tab.allow_unsafe_images.get(),
                                );
                            }
                        }
                    }
                }
                if let Some(src) = src_weak.and_then(|w| w.upgrade()) {
                    // A window with zero tabs is never a
                    // valid state — close it.
                    if winstate::tab_count(&src) == 0 {
                        src.close();
                    } else {
                        update_window_title(&src);
                    }
                }
            });
        }
    ));
}

/// widgets/tab: the ONE `GtkDragSource`/`GtkDropTarget` pair, attached
/// directly to `tab_view.bar_widget()` (the strip, not any individual tab),
/// that drives in-strip reorder AND cross-window move AND drag-to-desktop.
/// Reuses the exact u64-tab-id-payload mechanism the shipped `GtkNotebook`
/// hybrid already proved correct (formerly `wire_tab_drag_source`/
/// `wire_notebook_drop_target`, one per tab label) — see `widgets/tab`'s
/// module doc for why this design needs no Shift-gate and no raw
/// `gdk_drag_begin`: there is exactly one drag mechanism, so there is nothing
/// for it to race.
///
/// In-strip reorder is driven by `connect_motion` firing on THIS SAME bar's
/// own `GtkDropTarget` while its own drag hovers over itself (a normal,
/// supported self-target case) — [`TabView::preview_reorder`] no-ops unless
/// the hovered content genuinely belongs to `tab_view`, so hovering another
/// window's bar during the same drag does nothing here.
pub(crate) fn wire_tab_bar_dnd(window: &ApplicationWindow, tab_view: &TabView) {
    let bar = tab_view.bar_widget();
    let dragging: Rc<RefCell<Option<gtk::Widget>>> = Rc::new(RefCell::new(None));

    // ── SOURCE half ──────────────────────────────────────────────────────────
    let source = gtk::DragSource::new();
    source.set_actions(gdk::DragAction::MOVE);
    // NB: every closure below captures a WEAK TabView (`tab_view.downgrade()`) and
    // upgrades at fire time. A strong `tab_view.clone()` stored in a DragSource/
    // DropTarget that is itself `add_controller`ed onto the bar forms the cycle
    // `bar → controller → closure → tv → tv.bar (== bar)`, which retained the whole
    // per-window tab UI (TabBar + its `tabs` Vec of SplitViews/editors) on every
    // window close (H2 — the dominant leak; a second instance of the same pattern
    // QA flagged in the `TabView::connect_*` cells).
    // NB: `tab_view` is a plain wrapper struct (custom `downgrade()` →
    // `WeakTabView`), not a glib GObject, so `glib::clone!`'s `#[weak]` (which
    // needs the glib `Downgrade` trait) does not apply here — this and the other
    // `tab_view`-capturing DnD closures keep the hand-rolled weak-capture idiom.
    {
        let wtv = tab_view.downgrade();
        let dragging = dragging.clone();
        source.connect_prepare(move |_src, x, y| {
            let tv = wtv.upgrade()?;
            let content = tv.content_at(x, y)?;
            let tab = winstate::tab_by_content_box(&content)?;
            *dragging.borrow_mut() = Some(content);
            // The GLib drag payload carries the raw u64 (TabId isn't a GType);
            // the drop side re-wraps it via `TabId::from_raw`.
            Some(gdk::ContentProvider::for_value(&tab.id.raw().to_value()))
        });
    }
    {
        let wtv = tab_view.downgrade();
        let dragging = dragging.clone();
        source.connect_drag_begin(move |_src, drag| {
            let Some(tv) = wtv.upgrade() else { return };
            let Some(content) = dragging.borrow().clone() else {
                return;
            };
            // Drag icon = the tab itself, frozen (widgets/tab §B). Capture and dim
            // are fused in `begin_drag_visuals` so the snapshot cannot be taken
            // after the dim — doing so clears the cached render node and yields an
            // empty paintable, i.e. no drag icon at all (ScrAP-173).
            if let Some(frozen) = tv.begin_drag_visuals(&content) {
                gtk::DragIcon::set_from_paintable(drag, &frozen, 0, 0);
            }
        });
    }
    {
        let wtv = tab_view.downgrade();
        let dragging = dragging.clone();
        source.connect_drag_end(move |_src, _drag, _delete| {
            let Some(tv) = wtv.upgrade() else { return };
            if let Some(content) = dragging.borrow_mut().take() {
                tv.set_dragging(&content, false);
                tv.settle_reorder();
                // A same-window reorder changes visual tab ORDER without any
                // open/close/move, so `update_window_title` never fires for it —
                // refresh this window's Documents list so its entries track the
                // strip. (A cross-window move / drag-to-desktop is additionally
                // covered by `update_window_title` on both windows via
                // `wire_tab_arrival`; the extra coalesced refresh here is a no-op.)
                if let Some(win) = tv
                    .widget()
                    .root()
                    .and_then(|r| r.downcast::<ApplicationWindow>().ok())
                {
                    refresh_documents_menu(&win);
                }
            }
        });
    }
    source.connect_drag_cancel(glib::clone!(
        #[weak(rename_to = w)]
        window,
        #[strong]
        dragging,
        #[upgrade_or]
        false,
        move |_src, _drag, reason| {
            let Some(content) = dragging.borrow().clone() else {
                return false;
            };
            if reason != gdk::DragCancelReason::NoTarget {
                return false;
            }
            let Some(tab) = winstate::tab_by_content_box(&content) else {
                return false;
            };
            let Some(app) = w.application() else {
                return false;
            };
            let source_chrome = tab.chrome();
            // `spawn_window_hosting_tab` already logs on the near-impossible
            // `None` path (QA round-1 L6) and leaves `tab.content_box`
            // untouched on it (QA round-2 R2-1) — report the cancel as
            // unhandled so GTK plays its normal snap-back animation instead
            // of claiming a detach that didn't happen. `w` is the window whose
            // tab bar started this drag, i.e. the source the pop-out inherits.
            spawn_window_hosting_tab(&app, &w, &source_chrome.tabs, &tab.content_box).is_some()
        }
    ));
    bar.add_controller(source);

    // ── DESTINATION half ─────────────────────────────────────────────────────
    let target = gtk::DropTarget::new(u64::static_type(), gdk::DragAction::MOVE);
    target.set_preload(true);
    {
        let wtv = tab_view.downgrade();
        target.connect_motion(move |t, x, _y| {
            let Some(tv) = wtv.upgrade() else {
                return gdk::DragAction::empty();
            };
            if let Some(id) = t
                .value()
                .and_then(|v| v.get::<u64>().ok())
                .map(winstate::TabId::from_raw)
            {
                if let Some(tab) = winstate::tab_by_id(id) {
                    tv.preview_reorder(tab.content_box.upcast_ref::<gtk::Widget>(), x);
                }
            }
            gdk::DragAction::MOVE
        });
    }
    // Captures a weak `tab_view` (non-GObject wrapper), so it stays hand-rolled
    // rather than `glib::clone!` — see the `connect_prepare` note above.
    {
        let win = window.downgrade();
        let wtv = tab_view.downgrade();
        target.connect_drop(move |_t, value, _x, _y| {
            let Some(tv) = wtv.upgrade() else {
                return false;
            };
            let Some(dest) = win.upgrade() else {
                return false;
            };
            let Some(dest_chrome) = winstate::chrome(&dest) else {
                return false;
            };
            let Ok(raw_id) = value.get::<u64>() else {
                return false;
            };
            let tab_id = winstate::TabId::from_raw(raw_id);
            let Some(tab) = winstate::tab_by_id(tab_id) else {
                return false;
            };
            let source_chrome = tab.chrome();
            if Rc::ptr_eq(&source_chrome, &dest_chrome) {
                // Dropped back onto its own window's strip — `preview_reorder`
                // already left it in the right order; just make sure the
                // animation runs to completion.
                tv.settle_reorder();
                return true;
            }
            // Same rule as the pop-out path above: the source window's switch to
            // whatever the departing tab exposes is not a navigation (TDD 23.9).
            // The source window is resolved from the tab's own live root — the
            // drop handler is handed chrome, not a window, and a `TabState`
            // deliberately holds no window reference (`winstate`'s module doc).
            match window_of_content_box(&tab.content_box) {
                Some(source) => {
                    let _no_history = winstate::nav_suppress(&source);
                    source_chrome.tabs.detach_tab(&tab.content_box);
                }
                None => source_chrome.tabs.detach_tab(&tab.content_box),
            }
            dest_chrome.tabs.append_page(&tab.content_box);
            // The tab-arrival callback (`wire_tab_arrival`) now owns every
            // remaining step — registry rehome, titles, and closing an
            // emptied source window — deferred exactly as it already was for
            // the explicit "Move Tab to New Window" action this reuses.
            true
        });
    }
    bar.add_controller(target);
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    /// Mirror-correctness property, for BOTH per-tab toggles that
    /// seed their `win.*` GAction to a hardcoded default at window
    /// construction (`show-unsafe-images` pre-existing, `allow-outside-links`
    /// new): Move Tab to New Window must not leave the destination window's
    /// checkbox/button reporting that construction-time default when the
    /// MOVED tab's own value disagrees with it.
    ///
    /// NOTE on this test's actual sensitivity, established by mutation-testing
    /// (temporarily removing `wire_tab_arrival`'s explicit
    /// `resync_tab_action_state` call and re-running this test): for BOTH
    /// toggles, this specific path — Move Tab to New Window — already
    /// self-heals via the natural `switch-page` signal. `spawn_bare_window_
    /// for_tab` leaves `active_idx` at `None` after removing the throwaway
    /// starter tab, so `focus_page`'s `switch_to_index` (`widgets/tab/ops.rs`)
    /// never hits its same-index no-op guard for the arriving tab — meaning
    /// neither toggle's action was ever actually observed to "lie" via THIS
    /// path in the current codebase (the plausible-from-reading-`WindowInit`
    /// diagnosis did not hold up under execution — GTK4Rs/AP-91's own
    /// lesson: verify a source-based theory by running it). The explicit
    /// resync call is kept anyway as defensive, enforced insurance (see its
    /// call site's comment) against a future change to `widgets/tab`'s
    /// index-bookkeeping silently reintroducing the no-op case. This test
    /// therefore currently asserts the END STATE (the property that must
    /// always hold, defended in depth), not that one specific line is what
    /// produces it.
    #[gtktest::test]
    fn move_tab_to_new_window_mirrors_the_tabs_own_toggle_state() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.dnd"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");
        let source = crate::window::new_window(&app, "IT", "# One\n\nbody", None);

        // Flip BOTH of the SOURCE tab's toggles ON — disagrees with any fresh
        // window's hardcoded-`false` construction-time default for either.
        let st = state(&source).expect("state registered after new_window");
        st.allow_outside_links.set(true);
        change_action_state(&source, "allow-outside-links", &true.to_variant());
        st.allow_unsafe_images.set(true);
        change_action_state(&source, "show-unsafe-images", &true.to_variant());

        move_tab_to_new_window(&source);

        // The arrival handler's resync runs inside an idle callback (deferred
        // deliberately — see `wire_tab_arrival`'s own comment); pump the
        // default main context until nothing more is immediately pending.
        let ctx = glib::MainContext::default();
        for _ in 0..200 {
            if !ctx.iteration(false) {
                break;
            }
        }

        // The moved tab landed in a brand-new SECOND window.
        let dest = app
            .windows()
            .into_iter()
            .filter_map(|w| w.downcast::<ApplicationWindow>().ok())
            .find(|w| w.as_ptr() != source.as_ptr())
            .expect("move_tab_to_new_window spawned a destination window");

        assert!(
            bool_action_state(&dest, "allow-outside-links", false),
            "the destination window's action must mirror the MOVED TAB's own \
             true value, not the new window's construction-time default"
        );
        assert!(
            bool_action_state(&dest, "show-unsafe-images", false),
            "same mirror requirement for the pre-existing show-unsafe-images \
             toggle"
        );

        dest.destroy();
        source.destroy();
    }

    /// Move Tab to New Window must detach the SOURCE window's format overlay
    /// from the editor it is handing away.
    ///
    /// The overlay is a `set_parent`-attached `GtkPopover`, and GTK does not
    /// auto-unparent those (ScrAP-90). The destination window then parents its
    /// OWN overlay onto the same editor via `retarget_format_overlay`, so the
    /// editor ends up with two popover children — one of which belongs to a
    /// window that no longer owns it.
    ///
    /// **Why this asserts the PARENTING and not a warning flood.** When such an
    /// editor is eventually disposed, `gtk_text_view_dispose` drains its children
    /// with `while ((child = get_first_child(view))) gtk_text_view_remove(...)`,
    /// and `gtk_text_view_remove` *warns and returns without unparenting* any
    /// child lacking `quark_text_view_child` qdata — so the loop hands back the
    /// same popover forever. On GTK 4.22.4 that is an unbounded spin (measured:
    /// 28 MB of `GtkPopover is not a child of GtkSourceView` in 30 s on Win32;
    /// 12.1 GB in 5 min on Quartz). On 4.6.9 the moved editor is simply never
    /// disposed while the stale parent persists, so the same defect costs
    /// nothing and is **structurally invisible** here.
    ///
    /// A flood-based or hang-based guard would therefore not merely be weak on
    /// this platform — it would be **vacuous**, because there is no flood to
    /// observe under any behaviour, correct or broken. Asserting the parenting
    /// catches the defect everywhere, including where it currently costs
    /// nothing: **this guard failed on 4.6.9/X11 before the fix above existed**,
    /// which is the point of its shape. The general rule — assert the STATE that
    /// constitutes the defect, never the SYMPTOM the platform you found it on
    /// happens to produce — is why this must not be "simplified" into a timeout
    /// or warning check later.
    ///
    /// **Why this drives `move_tab_to_new_window` DIRECTLY rather than the
    /// `win.move-tab-new-window` action, and why that is not laziness.** The action is
    /// disabled for a window's only tab (TDD 15.4), so driving it can never produce the
    /// state under test: a surviving tab becomes active, `retarget_format_overlay`
    /// fires, and its popdown→unparent→set_parent pulls the overlay off the moved editor
    /// as a SIDE EFFECT — the stale parent never forms, and the check reports "ok" with
    /// the fix removed. That was measured on the macOS port, where a check built on the
    /// action passed with `detach_overlay_from` commented out.
    ///
    /// The path that genuinely reaches this state is the **drag-to-desktop pop-out**
    /// (TDD 7.7), which has no single-tab restriction and so can strip a window of its
    /// last tab with nothing left to trigger that incidental rescue. That path does not
    /// call `move_tab_to_new_window` at all — both funnel through
    /// `spawn_window_hosting_tab`, which is where the fix lives and what this test
    /// reaches by the shortest headlessly-drivable route.
    ///
    /// **So do not "modernise" this to drive the action.** It would still pass, and it
    /// would prove nothing.
    ///
    /// Mutation-tested in both directions on 4.6.9/X11: with the
    /// `detach_overlay_from` call removed from `spawn_window_hosting_tab`, this
    /// test fails on the final assertion; with it restored, it passes.
    #[gtktest::test]
    fn move_tab_to_new_window_detaches_the_source_windows_format_overlay() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.integrationtest.dndoverlay"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE)
            .expect("register (emits startup) before building any window");
        let source = crate::window::new_window(&app, "IT", "# One\n\nbody", None);

        let st = state(&source).expect("state registered after new_window");
        let moved_editor: gtk::Widget = st.editor.clone().upcast();
        let source_chrome = winstate::chrome(&source).expect("source window chrome");

        // Precondition: the source window's overlay really is on this editor, so
        // a pass below means "detached", not "was never attached".
        retarget_format_overlay(&st);
        assert_eq!(
            source_chrome.format_overlay.parent().as_ref(),
            Some(&moved_editor),
            "precondition: the source overlay must start out parented on the \
             editor that is about to be moved"
        );

        move_tab_to_new_window(&source);

        let ctx = glib::MainContext::default();
        for _ in 0..200 {
            if !ctx.iteration(false) {
                break;
            }
        }

        assert_ne!(
            source_chrome.format_overlay.parent().as_ref(),
            Some(&moved_editor),
            "the SOURCE window's format overlay is still parented on the editor \
             handed to another window; that editor can no longer be disposed \
             without wedging gtk_text_view_dispose's drain loop"
        );

        for w in app.windows() {
            w.destroy();
        }
    }
}
