//! [`TabView`] — the `GtkNotebook`-shaped Rust façade pairing the [`TabBar`]
//! strip with a `GtkStack` of tab bodies — and [`WeakTabView`], its weak
//! counterpart used by the `connect_*` closures to avoid the H2 reference cycle.

use super::*;

/// A `GtkNotebook`-shaped façade pairing [`TabBar`] (the strip, including its
/// own prev/next chevrons and its own self-owned `GtkAdjustment` — see the
/// module doc for why it is a plain child here, not wrapped in a
/// `GtkScrolledWindow`) with a `GtkStack` (tab bodies). Cheap to clone (every
/// field is a reference-counted GObject handle, or — for [`TabBar`] — a
/// wrapper around one whose own interior state is `RefCell`-shared across
/// every clone), matching how `WindowChrome`'s other widget fields are
/// already handled.
///
/// See the module doc for the deliberate simplification versus
/// the retired tab-widget plan's literal recipe, and for why the chevrons live inside
/// `TabBar` rather than as external `GtkBox` siblings.
///
/// **Tried and reverted: lifting the tab strip out from under the content
/// `GtkOverlay`** (splitting this struct into separate `bar_widget()` /
/// `stack_widget()` parents, per the researcher's tree-topology diagnosis of
/// ScrAP-56). Live-tested and found to make the residual warning
/// WORSE, not better: the warning still reproduced (same `Trying to snapshot
/// ... without a current allocation` mechanism, now naming whatever new
/// widget became the nearest shared ancestor — a plain `GtkBox` instead of
/// `GtkOverlay`), AND the resulting stuck-blank window grew from a
/// single-frame, always-self-healing flicker to a full-second-plus stuck
/// blank that also swallowed the toolbar and tab strip, not just the content
/// pane, requiring a second click to recover. Reverted back to this combined
/// widget. See ScrAP-56 round 4 for the full account, including the
/// researcher's own residual-cause (c): a NEWLY-switched-to tab's content
/// pane can trigger its own first-layout `queue_resize` (lazy
/// `GtkSourceView`/`GtkTextView` line-height validation) independent of
/// anything the tab strip does — this is likely the REAL trigger (the repro
/// that found it required zero strip scrolling), which tree-topology
/// surgery around the strip was never going to touch.
#[derive(Clone)]
pub(crate) struct TabView {
    widget: gtk::Box,
    bar: TabBar,
    stack: gtk::Stack,
}

/// A weak counterpart of [`TabView`] (H2). The `connect_*` façade closures below
/// capture THIS, never a strong `TabView`. A strong `self.clone()` stored back
/// into the `TabBar`'s own imp callback cells forms the reference cycle
/// `bar → imp.<cb> → closure → tv → tv.bar (== bar)`. In GTK4 a widget's
/// `dispose`/finalize only runs once its refcount reaches 0, so such a cycle
/// keeps the whole per-window tab UI (TabBar + GtkStack + every tab body/buffer)
/// alive forever after the window closes — and clearing the cells inside
/// `TabBar::dispose` cannot rescue it, because the cycle is exactly what stops
/// `dispose` from ever being called (empirically confirmed: on window destroy
/// `winstate::unregister` runs but the TabBar's dispose/finalize never does — see
/// ScrAP-60). Capturing weak and upgrading at fire time means the cycle
/// never forms, so the TabBar finalizes normally when the window is destroyed.
#[derive(Clone)]
pub(crate) struct WeakTabView {
    widget: glib::WeakRef<gtk::Box>,
    bar: glib::WeakRef<TabBar>,
    stack: glib::WeakRef<gtk::Stack>,
}

impl WeakTabView {
    pub(crate) fn upgrade(&self) -> Option<TabView> {
        Some(TabView {
            widget: self.widget.upgrade()?,
            bar: self.bar.upgrade()?,
            stack: self.stack.upgrade()?,
        })
    }
}

impl TabView {
    pub(crate) fn downgrade(&self) -> WeakTabView {
        WeakTabView {
            widget: self.widget.downgrade(),
            bar: self.bar.downgrade(),
            stack: self.stack.downgrade(),
        }
    }

    pub(crate) fn new() -> Self {
        let bar = TabBar::new();
        bar.set_hexpand(true);

        let stack = gtk::Stack::new();
        stack.set_vexpand(true);
        // Non-homogeneous (both axes) is load-bearing for first-show paint, NOT
        // a cosmetic sizing choice. GtkStack's default homogeneous mode makes
        // `set_visible_child` issue only a NON-bubbling `queue_allocate` on the
        // stack (gtkstack.c:1364-1367): the shared content `GtkOverlay` ancestor
        // is skip-allocated and merely descended through. When a never-before-
        // shown tab body (a `CodePreviewView`/`GtkTextView` subtree) then does
        // its lazy first-allocation line-height validation, it dirties the
        // overlay AFTER the descent has already passed it — with no event-phase
        // `queue_resize` having scheduled a heal — so GTK snapshot-skips the
        // overlay ("Trying to snapshot GtkOverlay … without a current
        // allocation") and, crucially, STAYS stuck-blank until an external
        // resize forces a relayout (the native window's post-layout re-check
        // saw nothing needing allocation, so scheduled no follow-up frame).
        // Non-homogeneous instead makes `set_visible_child` issue a bubbling
        // `queue_resize(stack)` from the EVENT phase, which propagates a real
        // `alloc_needed` + render request up through the overlay to the native
        // window BEFORE layout runs → overlay is allocated-then-painted in order
        // (no skip, no blank). Because the preview `ScrolledWindow` is
        // vertically decoupled (Always vbar + propagate_natural_height=FALSE),
        // the subsequent idle validation correction is absorbed there as a mere
        // scroll-range change, so non-homogeneous does NOT cause a visible
        // reflow. Keep the default transition-type=NONE: a crossfade/slide
        // re-arms the overlay skip every animation frame via queue_draw and
        // makes the blank WORSE (researcher-verified, gtkwidget.c:3541-3552).
        // See ScrAP-56 round 5 for the full account and the two prior
        // (homogeneous) rounds this reverses. (gtk-rs default is homogeneous.)
        stack.set_hhomogeneous(false);
        stack.set_vhomogeneous(false);

        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.append(&bar);
        widget.append(&stack);

        let tv = TabView { widget, bar, stack };

        {
            // Keep the GtkStack's visible child correct on EVERY switch,
            // including one driven purely internally by the strip's own
            // primary-click gesture (`TabBar::new`'s `GtkGestureClick`) —
            // that path never goes through `TabView::set_current_page`, so
            // without this the stack silently kept showing the previously
            // active tab's content while every other piece of per-tab state
            // (outline, dirty status, find bar, …) correctly moved on (bug
            // found by live Xvfb testing during the tab-widget rollout).
            let stack = tv.stack.clone();
            tv.bar.set_internal_switch_cb(move |content, _idx| {
                stack.set_visible_child(content);
            });
        }
        tv
    }

    /// The whole assembled widget (strip + stack) — what call sites place
    /// into the content overlay slot in place of the old `GtkNotebook`.
    pub(crate) fn widget(&self) -> &gtk::Box {
        &self.widget
    }

    // ── GtkNotebook-shaped facade (the retired tab-widget plan §API surface) ─────────

    pub(crate) fn n_pages(&self) -> u32 {
        self.bar.n_tabs() as u32
    }

    pub(crate) fn current_page(&self) -> Option<u32> {
        self.bar.active_index().map(|i| i as u32)
    }

    pub(crate) fn set_current_page(&self, idx: Option<u32>) {
        let Some(idx) = idx else { return };
        // The GtkStack sync happens inside `switch_to_index` itself (via the
        // internal callback wired in `new`) — the SAME path the strip's own
        // primary-click gesture drives, so there is exactly one place that
        // can leave the stack showing the wrong tab, not two that could drift.
        self.bar.switch_to_index(idx as usize);
    }

    pub(crate) fn page_num(&self, child: &impl IsA<gtk::Widget>) -> Option<u32> {
        self.bar
            .index_of(child.upcast_ref::<gtk::Widget>())
            .map(|i| i as u32)
    }

    /// Every tab's content widget in visual strip order (see
    /// `TabBar::ordered_contents`). The View ▸ Documents menu is built from this
    /// so its item order tracks the strip, not registry insertion order.
    pub(crate) fn ordered_contents(&self) -> Vec<gtk::Widget> {
        self.bar.ordered_contents()
    }

    /// Make the page hosting `child` the current (active) page. Returns `true`
    /// if `child` was found and selected. Consolidates the "look up the page
    /// number, then set it current" two-step that recurred at ~9 call sites
    /// (M6) — the façade already owns both halves, so no caller should open-code
    /// the pair.
    pub(crate) fn focus_page(&self, child: &impl IsA<gtk::Widget>) -> bool {
        match self.page_num(child) {
            Some(page_num) => {
                self.set_current_page(Some(page_num));
                true
            }
            None => false,
        }
    }

    pub(crate) fn append_page(&self, child: &impl IsA<gtk::Widget>) {
        let widget = child.upcast_ref::<gtk::Widget>().clone();
        self.stack.add_child(&widget);
        self.bar.add_tab(&widget);
        let idx = self.bar.n_tabs() as u32 - 1;
        self.bar.fire_page_added(&widget, idx);
    }

    pub(crate) fn remove_page(&self, idx: Option<u32>) {
        let Some(idx) = idx else { return };
        if let Some(content) = self.bar.remove_at(idx as usize) {
            self.stack.remove(&content);
        }
    }

    /// Mark the window's initial (index 0) tab active without firing a switch — see
    /// `TabBar::mark_first_active`. Called once by `build_chrome` after appending
    /// the first page, so `active_idx`/`current_page` reflect the default-visible
    /// first tab (which never goes through `switch_to_index`).
    pub(crate) fn mark_first_active(&self) {
        self.bar.mark_first_active();
    }

    pub(crate) fn detach_tab(&self, child: &impl IsA<gtk::Widget>) {
        let widget = child.upcast_ref::<gtk::Widget>().clone();
        self.bar.remove_by_content(&widget);
        self.stack.remove(&widget);
    }

    /// Set `child`'s tab-strip label from a **Pango markup** string (the label
    /// carries the coloured "⚠" deleted-backing badge, so it must be markup, not
    /// plain text — the caller in `window/tabs/documents.rs` escapes the
    /// filename before interpolating it).
    pub(crate) fn set_tab_markup(&self, child: &impl IsA<gtk::Widget>, markup: &str) {
        self.bar
            .set_markup(child.upcast_ref::<gtk::Widget>(), markup);
    }

    pub(crate) fn set_tab_tooltip(&self, child: &impl IsA<gtk::Widget>, text: &str) {
        self.bar
            .set_tooltip(child.upcast_ref::<gtk::Widget>(), text);
    }

    /// Show/hide a tab's leading busy spinner (a deferred tab awaiting its first
    /// preview render). Set on when the deferred tab is created, off when it
    /// materializes — see `window::tabs`.
    pub(crate) fn set_tab_busy(&self, child: &impl IsA<gtk::Widget>, busy: bool) {
        self.bar.set_busy(child.upcast_ref::<gtk::Widget>(), busy);
    }

    /// Whether `child`'s tab currently shows its busy spinner (for tests).
    #[cfg(all(test, feature = "gtk-integration-tests"))]
    pub(crate) fn tab_busy(&self, child: &impl IsA<gtk::Widget>) -> bool {
        self.bar.is_busy(child.upcast_ref::<gtk::Widget>())
    }

    // Each façade closure captures a WEAK TabView and upgrades at fire time — a
    // strong capture would cycle through the TabBar's own callback cells and leak
    // the whole tab UI on window close (H2; see [`WeakTabView`]).
    pub(crate) fn connect_switch_page(&self, f: impl Fn(&TabView, &gtk::Widget, u32) + 'static) {
        let wtv = self.downgrade();
        self.bar.connect_switch_page(move |content, idx| {
            if let Some(tv) = wtv.upgrade() {
                f(&tv, content, idx);
            }
        });
    }

    pub(crate) fn connect_page_added(&self, f: impl Fn(&TabView, &gtk::Widget, u32) + 'static) {
        let wtv = self.downgrade();
        self.bar.connect_page_added(move |content, idx| {
            if let Some(tv) = wtv.upgrade() {
                f(&tv, content, idx);
            }
        });
    }

    pub(crate) fn connect_tab_close_requested(&self, f: impl Fn(&TabView, &gtk::Widget) + 'static) {
        let wtv = self.downgrade();
        self.bar.connect_close_requested(move |content| {
            if let Some(tv) = wtv.upgrade() {
                f(&tv, content);
            }
        });
    }

    pub(crate) fn connect_tab_context_menu(
        &self,
        f: impl Fn(&TabView, &gtk::Widget, f64, f64) + 'static,
    ) {
        let wtv = self.downgrade();
        self.bar.connect_context_menu(move |content, x, y| {
            if let Some(tv) = wtv.upgrade() {
                f(&tv, content, x, y);
            }
        });
    }

    // ── reorder / cross-window drag support (wired by `window/tabs/`'s
    // `wire_tab_bar_dnd`) ────────────────────────────────────────────────────

    /// The strip's own widget — attach the shared `GtkDragSource`/
    /// `GtkDropTarget` pair here (its CSS node name is `tabbar`, matching the
    /// `tabbar:drop(active)` rule in `preview.rs`).
    pub(crate) fn bar_widget(&self) -> gtk::Widget {
        self.bar.clone().upcast()
    }

    pub(crate) fn content_at(&self, x: f64, y: f64) -> Option<gtk::Widget> {
        self.bar.content_at(x, y)
    }

    pub(crate) fn set_dragging(&self, content: &gtk::Widget, dragging: bool) {
        self.bar.set_dragging(content, dragging);
    }

    /// Capture-then-dim for a drag start; see [`TabBar::begin_drag_visuals`].
    ///
    /// There is deliberately no `handle_widget` accessor here: it existed only to
    /// let a caller snapshot the handle itself, which is the sequence ScrAP-173 is
    /// about. Routing drag-start visuals through this one call makes the broken
    /// ordering unrepresentable from outside `widgets::tab`.
    pub(crate) fn begin_drag_visuals(&self, content: &gtk::Widget) -> Option<gdk::Paintable> {
        self.bar.begin_drag_visuals(content)
    }

    pub(crate) fn preview_reorder(&self, dragged: &gtk::Widget, hover_x: f64) {
        self.bar.preview_reorder(dragged, hover_x);
    }

    pub(crate) fn settle_reorder(&self) {
        self.bar.settle_reorder();
    }
}
