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
/// GTK4Rs/AP-104). Live-tested and found to make the residual warning
/// WORSE, not better: the warning still reproduced (same `Trying to snapshot
/// ... without a current allocation` mechanism, now naming whatever new
/// widget became the nearest shared ancestor — a plain `GtkBox` instead of
/// `GtkOverlay`), AND the resulting stuck-blank window grew from a
/// single-frame, always-self-healing flicker to a full-second-plus stuck
/// blank that also swallowed the toolbar and tab strip, not just the content
/// pane, requiring a second click to recover. Reverted back to this combined
/// widget. See GTK4Rs/AP-104 round 4 for the full account, including the
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
/// GTK4Rs/AP-63). Capturing weak and upgrading at fire time means the cycle
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

/// Run `change`, a change to which page the stack shows, with GTK's
/// `gtk-label-select-on-focus` setting off.
///
/// When the stack shows a page again it gives the keyboard focus back to the widget
/// that last had it on that page (GTK 4.6.9 `gtkstack.c:1333-1338`, GTK4Rs/AP-356).
/// A selectable `GtkLabel` that gets the focus outside a click selects all of its text
/// (`gtklabel.c` `gtk_label_grab_focus`, GTK4Rs/AP-107). Every table cell is a
/// selectable label, so following a link in a cell and coming back selected the whole
/// cell. The setting is read once per grab, so turning it off for this one
/// synchronous call leaves the label's own selection as the reader left it. It is
/// handed back to the platform afterwards rather than set to `true`, so the app never
/// pins a value the desktop owns.
fn without_label_select_on_focus(stack: &gtk::Stack, change: impl FnOnce()) {
    let settings = stack.settings();
    if !settings.is_gtk_label_select_on_focus() {
        change();
        return;
    }
    settings.set_gtk_label_select_on_focus(false);
    change();
    settings.reset_property("gtk-label-select-on-focus");
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
        // See GTK4Rs/AP-104 round 5 for the full account and the two prior
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
                without_label_select_on_focus(&stack, || stack.set_visible_child(content));
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
            without_label_select_on_focus(&self.stack, || self.stack.remove(&content));
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
        without_label_select_on_focus(&self.stack, || self.stack.remove(&widget));
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

    /// `child`'s tab-strip label, markup resolved to plain text (for tests) — the
    /// observable of [`set_tab_markup`](Self::set_tab_markup).
    #[cfg(all(test, feature = "gtk-integration-tests"))]
    pub(crate) fn tab_label_text(&self, child: &impl IsA<gtk::Widget>) -> Option<String> {
        self.bar.label_text(child.upcast_ref::<gtk::Widget>())
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
    /// `:drop(active)` glow rule in `preview.rs`).
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
    /// let a caller snapshot the handle itself, which is the sequence GTK4Rs/AP-156 is
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

/// GTK-object tests: the stack hands focus back only inside a mapped toplevel, and a
/// label selects on focus only there too (GTK4Rs/AP-124), so these present a window.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    fn pump_until(cond: impl FnMut() -> bool) -> bool {
        crate::testpump::until_or_for(
            crate::testpump::Clock::Idle,
            std::time::Duration::from_secs(5),
            cond,
        )
    }

    /// Coming back to a tab whose focus was in a table cell leaves the cell's
    /// selection as it was, instead of selecting the whole cell. Follows a link in a
    /// cell to another document and returns with Back (TDD 2.9a).
    #[gtktest::test]
    fn returning_to_a_tab_does_not_select_the_focused_cell() {
        let tv = TabView::new();
        let page_a = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let cell = gtk::Label::new(Some("alpha bravo"));
        crate::widgets::table::make_cell_selectable(&cell);
        page_a.append(&cell);
        let page_b = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let other = gtk::Button::with_label("other document");
        page_b.append(&other);
        tv.append_page(&page_a);
        tv.mark_first_active();
        tv.append_page(&page_b);

        let win = gtk::Window::new();
        win.set_default_size(300, 200);
        win.set_child(Some(tv.widget()));
        win.present();
        assert!(
            pump_until(|| cell.is_mapped()),
            "no display: the window never mapped"
        );

        // The reader clicked a link in the cell: focus there, nothing selected.
        cell.grab_focus();
        cell.select_region(0, 0);
        assert_eq!(cell.selection_bounds(), None);

        tv.set_current_page(Some(1));
        other.grab_focus();
        tv.set_current_page(Some(0));

        let focus = GtkWindowExt::focus(&win);
        assert_eq!(
            focus.as_ref(),
            Some(cell.upcast_ref::<gtk::Widget>()),
            "precondition: the stack gave the focus back to the cell"
        );
        assert_eq!(cell.selection_bounds(), None, "the whole cell was selected");
        assert!(
            cell.settings().is_gtk_label_select_on_focus(),
            "the setting was not handed back"
        );

        // Positive control: the same return through the bare stack selects the cell,
        // so the assertion above can fail.
        tv.set_current_page(Some(1));
        other.grab_focus();
        tv.stack.set_visible_child(&page_a);
        assert_eq!(cell.selection_bounds(), Some((0, 11)));
        cell.select_region(0, 0);
        win.destroy();
    }
}
