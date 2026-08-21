//! [`TabBar`] construction, tab bookkeeping (lookups / hit-testing), and the
//! callback-registration surface shared across every `TabView` clone. The
//! structural mutations (add / remove / switch / reorder) and the sibling-slide
//! animation live in the sibling [`super::ops`] module; the pure hit-test /
//! layout arithmetic in [`super::layout`].

use super::layout;
use super::*;
use crate::icons::Icon;

/// How far one chevron click scrolls the strip (px).
const CHEVRON_SCROLL_STEP: f64 = 120.0;
/// Pixels scrolled per unit of vertical wheel motion over the strip.
const WHEEL_SCROLL_STEP: f64 = 40.0;

/// Whether a bar-local press at (`x`, `y`) landed on (or inside) a tab's `×`
/// close button — resolved by [`WidgetExt::pick`] against the actual laid-out
/// widget tree and walking ancestors for the `tab-close-btn` CSS class the
/// button carries. Used to suppress the bar's tab-activate on such presses
/// (see AP for the container-gesture-fires-on-child-button pitfall).
fn press_hit_close_button(bar: &TabBar, x: f64, y: f64) -> bool {
    widget_or_ancestor_has_class(bar.pick(x, y, gtk::PickFlags::DEFAULT), TAB_CLOSE_BTN_CLASS)
}

/// Whether `w` or any of its ancestors carries the CSS class `class`. The
/// load-bearing half of [`press_hit_close_button`] (the GTK4Rs/AP-109 guard behind
/// TDD 7.11): a press lands on the close button's inner icon/label, so the class
/// is found one or more hops UP the tree, not on the picked leaf. Split out from
/// the `pick` plumbing so the ancestor-walk decision is unit-testable (QA H-4).
fn widget_or_ancestor_has_class(mut w: Option<gtk::Widget>, class: &str) -> bool {
    while let Some(cur) = w {
        if cur.has_css_class(class) {
            return true;
        }
        w = cur.parent();
    }
    false
}

impl TabBar {
    pub(super) fn new() -> Self {
        let obj: Self = glib::Object::new();
        // Overflow::Hidden clips handles/chevrons allocated outside
        // [0, width) — both the ordinary "scrolled past the viewport" case
        // (now driven by `hadjustment`, not a private offset) and the
        // "chevron currently hidden" case (GTK4Rs/AP-104).
        obj.set_overflow(gtk::Overflow::Hidden);

        // `TabBar` creates and owns its own `hadjustment` — deliberately NOT
        // supplied by a wrapping `GtkScrolledWindow` (see the module doc's
        // "tried and reverted" paragraph). Routing it through
        // `ScrollableExt::set_hadjustment` (rather than poking the `RefCell`
        // directly) reuses the exact same property-setter code path an
        // external `GtkScrolledWindow` would have used, so `TabBar` remains
        // a fully conformant `Scrollable` a future caller could still wrap
        // in a real one without any code change here.
        let hadj = gtk::Adjustment::new(0.0, 0.0, 0.0, 1.0, 1.0, 0.0);
        obj.set_hadjustment(Some(&hadj));

        let prev_btn = gtk::Button::from_icon_name(Icon::GoPrevious.name());
        prev_btn.add_css_class("flat");
        crate::a11y::name(&prev_btn, "Scroll tabs left");
        // Start insensitive, matching the not-shown default (`prev_shown` = false):
        // `size_allocate` only calls `set_sensitive` when the shown-state *changes*,
        // so on a strip that NEVER overflows it would otherwise leave the shoved-off
        // chevron at GTK's default sensitive=true → keyboard-focusable offscreen (QA
        // L-1, a11y). Overflow flips it sensitive on the first change.
        prev_btn.set_sensitive(false);
        prev_btn.set_parent(&obj);
        prev_btn.connect_clicked(glib::clone!(
            #[weak(rename_to = bar)]
            obj,
            move |_| {
                bar.scroll_by(-CHEVRON_SCROLL_STEP);
            }
        ));
        *obj.imp().prev_btn.borrow_mut() = Some(prev_btn);

        let next_btn = gtk::Button::from_icon_name(Icon::GoNext.name());
        next_btn.add_css_class("flat");
        crate::a11y::name(&next_btn, "Scroll tabs right");
        // Insensitive by default, same as prev_btn above (QA L-1).
        next_btn.set_sensitive(false);
        next_btn.set_parent(&obj);
        next_btn.connect_clicked(glib::clone!(
            #[weak(rename_to = bar)]
            obj,
            move |_| {
                bar.scroll_by(CHEVRON_SCROLL_STEP);
            }
        ));
        *obj.imp().next_btn.borrow_mut() = Some(next_btn);

        let click = gtk::GestureClick::new();
        click.set_button(1);
        click.connect_pressed(glib::clone!(
            #[weak(rename_to = bar)]
            obj,
            move |_g, _n_press, x, y| {
                // A press that lands on a tab's × close button must NOT first
                // activate that tab (TDD 7.11: closing a background tab keeps
                // whichever tab was already active). This bar-level gesture
                // fires on any press inside a handle — including on its close
                // button child — so the button's own `clicked` would then close
                // a tab we just made active, handing active to its neighbour.
                // Pick the real target and bail if it's (inside) a close button.
                if press_hit_close_button(&bar, x, y) {
                    return;
                }
                if let Some(idx) = bar.index_at(x, y) {
                    bar.switch_to_index(idx);
                }
            }
        ));
        obj.add_controller(click);

        let right_click = gtk::GestureClick::new();
        right_click.set_button(3);
        right_click.connect_pressed(glib::clone!(
            #[weak(rename_to = bar)]
            obj,
            move |g, _n_press, x, y| {
                g.set_state(gtk::EventSequenceState::Claimed);
                if let Some(idx) = bar.index_at(x, y) {
                    let content = bar.imp().tabs.borrow()[idx].content.clone();
                    if let Some(cb) = bar.imp().menu_cb.borrow().as_ref() {
                        cb(&content, x, y);
                    }
                }
            }
        ));
        obj.add_controller(right_click);

        // §G wheel requirement: vertical wheel motion over the strip scrolls
        // it horizontally (mouse-only is not distinguished here — touchpad
        // users get the same mapping). There is no wrapping `GtkScrolledWindow`
        // to supply this for free (see the module doc), so `TabBar` owns it
        // directly.
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        scroll.connect_scroll(glib::clone!(
            #[weak(rename_to = bar)]
            obj,
            #[upgrade_or]
            glib::Propagation::Stop,
            move |_c, _dx, dy| {
                bar.scroll_by(dy * WHEEL_SCROLL_STEP);
                glib::Propagation::Stop
            }
        ));
        obj.add_controller(scroll);

        obj
    }

    // ── tab bookkeeping ─────────────────────────────────────────────────────

    pub(super) fn n_tabs(&self) -> usize {
        self.imp().tabs.borrow().len()
    }

    pub(super) fn index_of(&self, content: &gtk::Widget) -> Option<usize> {
        self.imp()
            .tabs
            .borrow()
            .iter()
            .position(|t| t.content.as_ptr() == content.as_ptr())
    }

    pub(super) fn content_of(&self, idx: usize) -> Option<gtk::Widget> {
        self.imp().tabs.borrow().get(idx).map(|t| t.content.clone())
    }

    /// Every tab's content widget in VISUAL STRIP order (left-to-right), which
    /// is the source of truth for order after a drag-reorder — unlike
    /// `winstate::tabs_for_window`, whose registry order is mere insertion order
    /// and diverges from the strip once a tab is reordered (the same
    /// index-vs-identity split that caused GTK4Rs/AP-74). Used to build the View ▸
    /// Documents menu so its entries match what the user sees.
    pub(super) fn ordered_contents(&self) -> Vec<gtk::Widget> {
        self.imp()
            .tabs
            .borrow()
            .iter()
            .map(|t| t.content.clone())
            .collect()
    }

    pub(super) fn active_index(&self) -> Option<usize> {
        self.imp().active_idx.get()
    }

    /// Apply `mutate` to `content`'s tab entry, where the mutation may change
    /// the handle's **natural width**, and re-derive the strip's resting slots
    /// afterwards ([`TabBar::handle_width_changed`]).
    ///
    /// The two halves are fused into one call so the second cannot be forgotten
    /// at a call site (the GTK4Rs/AP-156 shape). A tab's x is the running total
    /// of the handles to its left, and GTK's own `queue_resize` re-runs the
    /// layout against the SAME stale slots — so a width change with no retarget
    /// leaves the strip drawing on a grid it no longer has, silently, until
    /// something unrelated (a close, a drag-reorder) happens to retarget it. A
    /// new width-changing setter added here inherits the fix by construction;
    /// one that reaches into `imp().tabs` on its own does not.
    ///
    /// **Enforcement tier: convention-only, deliberately** (POLICY § Typed GTK
    /// seams asks for this to be stated rather than left implicit). There is no
    /// method to ban — `set_markup`/`set_visible` are legitimate on any widget,
    /// so a `clippy.toml` entry would be all false positives — and the entry's
    /// fields must stay `pub(super)` for `ops::add_tab` to build them, so the
    /// bypass cannot be made non-compiling without restructuring the imp. What
    /// is bought instead is that the whole surface is two functions long and
    /// both are here: `set_markup` and `set_busy`, immediately below.
    fn with_entry_width_change(&self, content: &gtk::Widget, mutate: impl FnOnce(&imp::TabEntry)) {
        // Funnel through `index_of` so the content→entry pointer-identity scan
        // lives in exactly one place (D2).
        let Some(idx) = self.index_of(content) else {
            return;
        };
        {
            let tabs = self.imp().tabs.borrow();
            let Some(entry) = tabs.get(idx) else { return };
            mutate(entry);
        }
        // The borrow above is released first: `handle_width_changed` measures
        // every handle and can re-enter this `RefCell`.
        self.handle_width_changed();
    }

    pub(super) fn set_markup(&self, content: &gtk::Widget, markup: &str) {
        self.with_entry_width_change(content, |t| {
            // Markup, not plain text: the label carries the coloured "⚠"
            // deleted-backing badge (the filename is escaped upstream). A
            // relabel resizes the handle — hence the width-change funnel.
            t.label.set_markup(markup);
        });
    }

    /// Show (and spin) or hide `content`'s tab's leading busy indicator — a
    /// deferred tab awaiting its first preview render. Toggling visibility
    /// changes the handle's natural width (a hidden GtkSpinner is skipped in
    /// `measure`), which is a POSITION change for every tab to its right — so
    /// this goes through the width-change funnel
    /// ([`with_entry_width_change`](Self::with_entry_width_change)), which
    /// re-derives the resting slots, and then `queue_resize` re-runs the strip
    /// layout. `GtkSpinner::start`/`stop` gate the frame-clock animation so a
    /// stopped, hidden spinner costs nothing.
    pub(super) fn set_busy(&self, content: &gtk::Widget, busy: bool) {
        self.with_entry_width_change(content, |t| {
            if busy {
                t.spinner.set_visible(true);
                t.spinner.start();
            } else {
                t.spinner.stop();
                t.spinner.set_visible(false);
            }
        });
        self.queue_resize();
    }

    /// Whether `content`'s tab is currently showing its busy spinner (a deferred
    /// tab awaiting first render). The observable of [`set_busy`](Self::set_busy),
    /// for tests.
    #[cfg(all(test, feature = "gtk-integration-tests"))]
    pub(super) fn is_busy(&self, content: &gtk::Widget) -> bool {
        self.index_of(content)
            .and_then(|idx| {
                self.imp()
                    .tabs
                    .borrow()
                    .get(idx)
                    .map(|t| t.spinner.is_visible())
            })
            .unwrap_or(false)
    }

    /// Set the hover tooltip for a tab handle (its full file path, or "Unsaved"
    /// for a pathless buffer). Applied to the whole handle box so the tooltip
    /// shows over the label and its padding; the close button keeps its own
    /// "Close tab" tooltip, which GTK resolves in preference when hovered.
    pub(super) fn set_tooltip(&self, content: &gtk::Widget, text: &str) {
        let Some(idx) = self.index_of(content) else {
            return;
        };
        if let Some(t) = self.imp().tabs.borrow().get(idx) {
            crate::a11y::describe(&t.handle, Some(text));
        }
    }

    pub(super) fn scroll_offset(&self) -> f64 {
        self.imp()
            .hadjustment
            .borrow()
            .as_ref()
            .map(|a| a.value())
            .unwrap_or(0.0)
    }

    /// Hit-test a bar-local point against the currently laid-out handles.
    /// `x` is in the SAME bar-local pixel space `size_allocate` allocates
    /// into (i.e. it includes whatever gutter is currently reserved for the
    /// chevrons) — subtracting `tabs_x0` converts it to the "logical"
    /// content-space `current_x`/`target_x` already use before adding back
    /// the scroll offset. The containing-span search itself is the pure
    /// [`layout::hit_index`].
    pub(super) fn index_at(&self, x: f64, _y: f64) -> Option<usize> {
        let imp = self.imp();
        // A click in either chevron gutter (outside the scrollable viewport)
        // must never resolve to a tab — even though a scrolled-under handle's
        // logical rect still covers that x — or a gutter click would switch to
        // a tab clipped out of view. The chevrons own the gutters.
        let x0 = imp.tabs_x0.get();
        if x < x0 || x >= x0 + imp.viewport_w.get() {
            return None;
        }
        let logical_x = (x - x0) + self.scroll_offset();
        let spans: Vec<layout::Span> = imp
            .tabs
            .borrow()
            .iter()
            .map(|t| layout::Span {
                start: t.current_x.get(),
                width: natural_width(&t.handle),
            })
            .collect();
        layout::hit_index(logical_x, &spans)
    }

    pub(super) fn content_at(&self, x: f64, y: f64) -> Option<gtk::Widget> {
        let idx = self.index_at(x, y)?;
        self.content_of(idx)
    }

    pub(super) fn handle_widget(&self, content: &gtk::Widget) -> Option<gtk::Widget> {
        let idx = self.index_of(content)?;
        self.imp()
            .tabs
            .borrow()
            .get(idx)
            .map(|t| t.handle.clone().upcast())
    }

    pub(super) fn set_dragging(&self, content: &gtk::Widget, dragging: bool) {
        let Some(idx) = self.index_of(content) else {
            return;
        };
        if let Some(t) = self.imp().tabs.borrow().get(idx) {
            t.handle.set_opacity(if dragging { 0.4 } else { 1.0 });
        }
    }

    /// Freeze the tab handle's current pixels for use as the drag icon, THEN dim
    /// the handle to mark the drag in flight.
    ///
    /// The two steps are fused into one call precisely so the order cannot be got
    /// wrong at the call site (GTK4Rs/AP-156). Dimming calls `set_opacity`, which
    /// issues a `queue_draw` that CLEARS the widget's cached render node walking
    /// to the root (`gtkwidget.c:3541-3552`). A `current_image()` taken in the
    /// same main-loop turn therefore finds no node and returns an **empty**
    /// paintable — which draws nothing on any backend, so the drag icon silently
    /// disappears. Capturing first avoids that entirely.
    ///
    /// The previous code had these as two separate calls in the wrong order, with
    /// a comment correctly describing the very hazard it then walked into — which
    /// is why they are welded together here rather than merely re-ordered.
    pub(super) fn begin_drag_visuals(&self, content: &gtk::Widget) -> Option<gdk::Paintable> {
        let handle = self.handle_widget(content)?;
        let frozen = gtk::WidgetPaintable::new(Some(&handle)).current_image();
        self.set_dragging(content, true);
        Some(frozen)
    }

    // ── callback registration (shared across every `TabView` clone — see
    // `TabView`'s doc comment) ──────────────────────────────────────────────

    pub(super) fn connect_switch_page(&self, f: impl Fn(&gtk::Widget, u32) + 'static) {
        *self.imp().switch_cb.borrow_mut() = Some(Box::new(f));
    }

    pub(super) fn connect_close_requested(&self, f: impl Fn(&gtk::Widget) + 'static) {
        *self.imp().close_cb.borrow_mut() = Some(Box::new(f));
    }

    pub(super) fn connect_context_menu(&self, f: impl Fn(&gtk::Widget, f64, f64) + 'static) {
        *self.imp().menu_cb.borrow_mut() = Some(Box::new(f));
    }

    pub(super) fn connect_page_added(&self, f: impl Fn(&gtk::Widget, u32) + 'static) {
        *self.imp().page_added_cb.borrow_mut() = Some(Box::new(f));
    }

    pub(super) fn fire_page_added(&self, content: &gtk::Widget, idx: u32) {
        if let Some(cb) = self.imp().page_added_cb.borrow().as_ref() {
            cb(content, idx);
        }
    }

    pub(super) fn set_internal_switch_cb(&self, f: impl Fn(&gtk::Widget, u32) + 'static) {
        *self.imp().internal_switch_cb.borrow_mut() = Some(Box::new(f));
    }
}

/// GTK-object tests: building widgets needs an initialized GTK, so — like
/// `window/reload.rs` — these use `#[gtktest::test]` behind the `gtk-integration-tests`
/// feature. No display is required (the tree is never realized/allocated).
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    /// TDD 7.11 / GTK4Rs/AP-109 regression (area-1 automated test): the tab-activate guard
    /// suppresses a press only when the picked widget — or ANY ancestor — carries
    /// the close-button class, because a click lands on the button's inner
    /// icon/label, not the button itself. This pins that ancestor-walk so the guard
    /// can't silently regress to a leaf-only check (which would reintroduce the
    /// background-tab-close active-loss). Uses the real tab-handle shape: an outer
    /// box (no class) → the close `Button` (class) → an inner child.
    #[gtktest::test]
    fn close_button_class_is_found_on_the_picked_widget_or_an_ancestor() {
        let class = crate::widgets::tab::TAB_CLOSE_BTN_CLASS;

        let outer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let btn = gtk::Button::new();
        btn.add_css_class(class);
        btn.set_parent(&outer);
        let inner = gtk::Label::new(Some("×"));
        btn.set_child(Some(&inner));

        // A press picked on the inner label resolves UP to the close button → hit.
        assert!(
            widget_or_ancestor_has_class(Some(inner.clone().upcast()), class),
            "the class must be found on an ancestor of the picked leaf"
        );
        // The button itself carries the class → hit.
        assert!(widget_or_ancestor_has_class(
            Some(btn.clone().upcast()),
            class
        ));
        // A press on the surrounding handle (no class, and the button is a
        // DESCENDANT, not an ancestor) → miss, so a plain tab press still activates.
        assert!(
            !widget_or_ancestor_has_class(Some(outer.clone().upcast()), class),
            "a press outside the close button must not be suppressed"
        );
        // A miss (nothing picked) is a miss.
        assert!(!widget_or_ancestor_has_class(None, class));

        // Detach the button from its parent box so no GTK finalize warning trails
        // the test (the button's own child is released with it).
        btn.unparent();
    }

    /// Does a `WidgetPaintable` freeze of `w` actually contain anything to draw?
    ///
    /// **The obvious check does not work.** An empty paintable still reports a
    /// full, plausible `intrinsic_width`/`intrinsic_height` (measured: 300x200 for
    /// a blanked 300x200 widget), so any assertion on its SIZE passes on the
    /// broken code and guards nothing. The only property that discriminates is
    /// whether snapshotting it produces a render node. Do not "simplify" this to a
    /// size check — that silently disarms the regression guard (GTK4Rs/AP-156).
    fn freeze_has_content(w: &impl IsA<gtk::Widget>) -> bool {
        let img = gtk::WidgetPaintable::new(Some(w.as_ref())).current_image();
        let snapshot = gtk::Snapshot::new();
        img.snapshot(
            &snapshot,
            img.intrinsic_width().max(1) as f64,
            img.intrinsic_height().max(1) as f64,
        );
        snapshot.to_node().is_some()
    }

    /// Pump the main loop until `cond` holds, bounded by a timeout SOURCE rather
    /// than a wall-clock check between iterations — the latter never fires on an
    /// idle display, so the loop would hang forever (GTK4Rs/AP-79).
    ///
    /// This citation said `GTK4Rs/AP-109` until QA round 3. GTK4Rs/AP-109 is a real entry
    /// about a container-level gesture also firing on a child button — which is
    /// what the OTHER two `GTK4Rs/AP-109` citations in this file correctly refer to,
    /// which is why a wrong one hid among them. The pump lesson is `GTK4Rs/AP-79` in the
    /// gtk4-rs SKILL and #88 in THIS register, and the prefix sweep that
    /// introduced `ScrAP-` rewrote the prefix without re-resolving the number.
    fn pump_until(cond: impl Fn() -> bool) -> bool {
        let expired = std::rc::Rc::new(std::cell::Cell::new(false));
        let e = expired.clone();
        glib::timeout_add_local_once(std::time::Duration::from_secs(5), move || e.set(true));
        while !cond() && !expired.get() {
            glib::MainContext::default().iteration(true);
        }
        cond()
    }

    /// A `TabBar` in a presented window, pumped until it has a real allocation.
    /// The strip's viewport and scroll geometry do not exist until a layout pass
    /// has run, and every assertion below is about drawn positions.
    fn presented_bar(width: i32) -> (gtk::Window, TabBar) {
        let bar = TabBar::new();
        bar.set_hexpand(true);
        let win = gtk::Window::new();
        win.set_default_size(width, 40);
        win.set_child(Some(&bar));
        win.present();
        pump_strip(&bar);
        (win, bar)
    }

    /// Add a tab the way the application does: the handle is created with an
    /// EMPTY label and titled afterwards, and `update_window_title` re-labels
    /// **every** tab in the strip on each open — so a handle's natural width is
    /// routinely changed after the tab exists, which is the whole subject here.
    fn add_titled_tab(bar: &TabBar, contents: &mut Vec<gtk::Widget>, title: &str) -> gtk::Widget {
        let content: gtk::Widget = gtk::Label::new(Some("tab body")).upcast();
        bar.add_tab(&content);
        contents.push(content.clone());
        let last = contents.len() - 1;
        for (i, existing) in contents.iter().enumerate() {
            let markup = if i == last {
                title.to_string()
            } else {
                format!("doc-{i}.md")
            };
            bar.set_markup(existing, &markup);
        }
        content
    }

    /// Pump WALL CLOCK until the sibling-slide animation has settled (its tick
    /// callback is gone). Turns are not time: the ease runs off the frame clock,
    /// so a tight `iteration(false)` loop would never advance it — and the loop
    /// is kept dispatchable on an idle display by a timeout SOURCE, never by a
    /// wall-clock check between iterations (GTK4Rs/AP-79).
    fn pump_strip(bar: &TabBar) {
        let ticks = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let counter = ticks.clone();
        let id = glib::timeout_add_local(std::time::Duration::from_millis(20), move || {
            counter.set(counter.get() + 1);
            glib::ControlFlow::Continue
        });
        while ticks.get() < 50 {
            glib::MainContext::default().iteration(true);
            // Settled once the animation has stopped — but only judged after a
            // few frames, since `tick_id` is also empty before one starts.
            if ticks.get() > 8 && bar.imp().tick_id.borrow().is_none() {
                break;
            }
        }
        id.remove();
    }

    /// Where each handle is actually drawn, in the strip's logical content
    /// space: its animated position and its current natural width.
    fn drawn_spans(bar: &TabBar) -> Vec<layout::Span> {
        bar.imp()
            .tabs
            .borrow()
            .iter()
            .map(|t| layout::Span {
                start: t.current_x.get(),
                width: natural_width(&t.handle),
            })
            .collect()
    }

    /// Assert no handle is drawn over the one to its left. Shared by the guards
    /// below so they state the same invariant the same way.
    fn assert_no_overlap(bar: &TabBar, expected_tabs: usize) {
        let spans = drawn_spans(bar);
        assert_eq!(spans.len(), expected_tabs);
        for pair in spans.windows(2) {
            let (left, right) = (pair[0], pair[1]);
            assert!(
                left.start + left.width <= right.start + 0.5,
                "tab handles overlap: {left:?} runs into {right:?} — all spans {spans:?}"
            );
        }
    }

    /// ISSUE-R regression (TDD 7.20, area-1 automated test): **a handle that
    /// changes width must move every tab after it.**
    ///
    /// This is the guard aimed squarely at the width-change funnel
    /// ([`TabBar::with_entry_width_change`]). A handle's width is not just its
    /// own business: a tab's x is the running total of the handles to its left,
    /// and the strip caches those slots (`target_x`) rather than deriving them
    /// per frame. A width change issues only GTK's `queue_resize`, which re-runs
    /// the layout against the SAME cached slots — so a handle that grows (here:
    /// a tab gaining its deferred-render busy spinner) is drawn straight over
    /// its right-hand neighbour until something else happens to retarget.
    #[gtktest::test]
    fn a_handle_that_grows_moves_the_tabs_after_it() {
        let (win, bar) = presented_bar(420);
        let mut contents = Vec::new();
        for i in 0..4 {
            add_titled_tab(&bar, &mut contents, &format!("doc-{i}.md"));
        }
        pump_strip(&bar);
        assert_no_overlap(&bar, 4);

        // The first tab is re-rendered in the background and gains its spinner:
        // its handle grows, and the three tabs after it must give way.
        bar.set_busy(&contents[0], true);
        pump_strip(&bar);
        assert_no_overlap(&bar, 4);
        win.destroy();
    }

    /// ISSUE-R regression (TDD 7.20, area-1 automated test): **a tab added to a
    /// strip whose handles changed width since it last settled must land clear
    /// of its left-hand neighbour.**
    ///
    /// The strip caches each handle's resting slot (`target_x`) and its animated
    /// position (`current_x`); a slot is the running total of the handles to its
    /// left, so a handle that changes width moves every slot after it. GTK's own
    /// `queue_resize` re-runs the layout against those SAME cached values, so a
    /// width change that does not retarget leaves the strip drawing on a grid it
    /// no longer has — and the next appended tab, whose slot IS re-derived, is
    /// drawn on top of the neighbour that never moved.
    ///
    /// The sequence mirrors a multi-file open exactly: each tab is added while
    /// the earlier ones show their deferred-render busy spinner, then they all
    /// materialise (each handle narrowing by its spinner), then one more
    /// document is opened into the settled strip.
    ///
    /// Asserted on the STATE (drawn spans), not on pixels: the overlap is
    /// computed layout, and a pixel assertion would additionally depend on the
    /// clip, the scroll offset and the theme.
    #[gtktest::test]
    fn a_tab_added_after_its_neighbours_changed_width_lands_clear_of_them() {
        let (win, bar) = presented_bar(420);
        let mut contents = Vec::new();
        for i in 0..8 {
            let content = add_titled_tab(&bar, &mut contents, &format!("doc-{i}.md"));
            // Every background tab of a multi-file open is deferred, and shows
            // its busy spinner from the moment it is added.
            bar.set_busy(&content, true);
        }
        pump_strip(&bar);

        // …and then they render, one after another, each handle losing its
        // spinner. THIS is the width change the strip used to ignore.
        for content in &contents {
            bar.set_busy(content, false);
        }
        pump_strip(&bar);

        add_titled_tab(&bar, &mut contents, "opened-later.md");
        pump_strip(&bar);

        assert_no_overlap(&bar, 9);
        win.destroy();
    }

    /// A display-wide CSS provider, removed again when this value drops.
    ///
    /// A provider added to the display is PROCESS-global state, and libtest runs
    /// the whole suite in one process — so it must be removed even if the test
    /// panics, or every later test renders under a restyled theme and fails
    /// somewhere unrelated (POLICY § Unit tests).
    struct DisplayCss(gtk::CssProvider);

    impl DisplayCss {
        fn install(css: &str) -> Option<Self> {
            let display = gdk::Display::default()?;
            let provider = gtk::CssProvider::new();
            provider.load_from_data(css);
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_USER,
            );
            Some(Self(provider))
        }
    }

    impl Drop for DisplayCss {
        fn drop(&mut self) {
            if let Some(display) = gdk::Display::default() {
                gtk::style_context_remove_provider_for_display(&display, &self.0);
            }
        }
    }

    /// ISSUE-R regression (TDD 7.20, area-1 automated test): **adding a tab
    /// re-derives the strip from the widths its handles have NOW.**
    ///
    /// This is the guard aimed at the second, independent mechanism — the
    /// retarget in `TabBar::add_tab` — and it exists because the width-change
    /// funnel cannot be the whole answer: a restyle changes every handle's width
    /// with no call into this widget at all (here a font-size change, which is
    /// what a theme switch does to the strip). The strip is then stale until
    /// *something* re-derives it, and adding a tab is the operation that must
    /// not be the one to draw on top of the mess.
    ///
    /// Mutation-testing note (GTK4Rs/AP-254): the funnel and this retarget are
    /// **independently sufficient** for the tab-added-after-a-width-change case,
    /// so neutering either one alone leaves that guard green. This test and
    /// `a_handle_that_grows_moves_the_tabs_after_it` are what separate them —
    /// each fails for exactly one mechanism. Neither is dead code.
    #[gtktest::test]
    fn a_tab_added_after_a_restyle_lands_clear_of_its_neighbours() {
        let (win, bar) = presented_bar(420);
        let mut contents = Vec::new();
        for i in 0..4 {
            add_titled_tab(&bar, &mut contents, &format!("doc-{i}.md"));
        }
        pump_strip(&bar);
        assert_no_overlap(&bar, 4);

        let _css = DisplayCss::install("tabbar .tab-handle label { font-size: 32px; }")
            .expect("a display is required for these tests");
        pump_strip(&bar);
        // The strip is legitimately stale HERE — nothing has told it the handles
        // grew — so no assertion is made about this moment. What must hold is
        // that the next tab operation re-derives the whole strip.

        // Added the bare way, deliberately: the application titles a new tab
        // immediately afterwards, and that relabel would route through the
        // width-change funnel and heal the strip on the funnel's behalf — which
        // would leave this test passing with `add_tab`'s own retarget deleted
        // (GTK4Rs/AP-254). Adding without a title exercises only the mechanism
        // this test is aimed at, and is a state the strip really does paint in
        // (a handle is born label-less; `update_window_title` runs after).
        let bare: gtk::Widget = gtk::Label::new(Some("tab body")).upcast();
        bar.add_tab(&bare);
        contents.push(bare);
        pump_strip(&bar);
        assert_no_overlap(&bar, 5);
        win.destroy();
    }

    /// ISSUE-R regression, second half (TDD 7.20): **switching to a just-added
    /// tab must actually reveal it**, even when the strip already overflows.
    ///
    /// `scroll_into_view` writes the adjustment, and every adjustment write is
    /// clamped into `[lower, upper - page_size]`. `upper` is published by
    /// `size_allocate`, which has not run yet for a tab appended in the same
    /// main-loop turn — so without republishing the range first, the reveal is
    /// silently cut short by exactly the new tab's own width and the tab stays
    /// clipped past the right edge, permanently (the next layout pass publishes
    /// the true range but never re-reveals).
    #[gtktest::test]
    fn switching_to_a_just_added_tab_scrolls_it_fully_into_view() {
        let (win, bar) = presented_bar(420);
        let mut contents = Vec::new();
        for i in 0..8 {
            add_titled_tab(&bar, &mut contents, &format!("doc-{i}.md"));
        }
        pump_strip(&bar);
        assert!(
            bar.imp().content_width.get() > bar.imp().viewport_w.get(),
            "precondition: the strip must already overflow, or the reveal is trivial"
        );

        add_titled_tab(&bar, &mut contents, "opened-later.md");
        bar.switch_to_index(contents.len() - 1);
        pump_strip(&bar);

        let spans = drawn_spans(&bar);
        let fresh = *spans.last().expect("the tab just added");
        let offset = bar.scroll_offset();
        let viewport = bar.imp().viewport_w.get();
        assert!(
            fresh.start - offset >= -0.5 && fresh.start + fresh.width - offset <= viewport + 0.5,
            "the newly added tab {fresh:?} is not fully inside the viewport \
             (offset {offset}, viewport {viewport})"
        );
        win.destroy();
    }

    /// GTK4Rs/AP-156 regression: the drag-icon freeze must be taken BEFORE the handle
    /// is dimmed. `set_opacity` issues a `queue_draw` that clears the widget's
    /// cached render node (`gtkwidget.c:3541-3552`), so a `current_image()` in the
    /// same main-loop turn returns an EMPTY paintable — which draws nothing on any
    /// backend, i.e. no drag icon at all.
    ///
    /// This pins the ordering invariant that `TabBar::begin_drag_visuals` encodes.
    /// The second half is a deliberate MUTATION of the order (GTK4Rs/AP-78: mutation-
    /// test the guard): it asserts the WRONG order really does produce an empty
    /// freeze, so a future refactor can't make this test vacuously pass.
    ///
    /// Needs a display (unlike the sibling test above) because a widget only has a
    /// render node once it has actually painted.
    #[gtktest::test]
    fn drag_icon_freeze_must_be_taken_before_the_handle_is_dimmed() {
        let win = gtk::Window::new();
        let handle = gtk::Label::new(Some("a tab handle"));
        win.set_child(Some(&handle));
        win.present();

        assert!(
            pump_until(|| freeze_has_content(&handle)),
            "precondition: the handle must paint at least once before it can be frozen"
        );

        // CORRECT ORDER — freeze first, then dim: the freeze has real content.
        let frozen_before_dim = freeze_has_content(&handle);
        handle.set_opacity(0.4);
        assert!(
            frozen_before_dim,
            "a freeze taken BEFORE the dim must contain something to draw"
        );

        // Restore and let it repaint, so the mutation below starts from a node.
        handle.set_opacity(1.0);
        assert!(
            pump_until(|| freeze_has_content(&handle)),
            "the handle must repaint after the opacity is restored"
        );

        // MUTATION — dim first, then freeze in the SAME turn: the freeze is empty.
        // If this ever starts passing, GTK's invalidation changed and the fix (and
        // this guard) need re-deriving; it must not be "fixed" by relaxing it.
        handle.set_opacity(0.4);
        assert!(
            !freeze_has_content(&handle),
            "a freeze taken AFTER the dim must be empty — if it is not, this guard \
             no longer discriminates and GTK4Rs/AP-156 must be re-verified"
        );

        win.destroy();
    }
}
