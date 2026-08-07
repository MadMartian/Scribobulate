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

    pub(super) fn set_markup(&self, content: &gtk::Widget, markup: &str) {
        // Funnel through `index_of` so the content→entry pointer-identity scan
        // lives in exactly one place (D2).
        let Some(idx) = self.index_of(content) else {
            return;
        };
        if let Some(t) = self.imp().tabs.borrow().get(idx) {
            // Markup, not plain text: the label carries the coloured "⚠"
            // deleted-backing badge (the filename is escaped upstream).
            t.label.set_markup(markup);
        }
    }

    /// Show (and spin) or hide `content`'s tab's leading busy indicator — a
    /// deferred tab awaiting its first preview render. Same `index_of`
    /// content→entry funnel as [`set_markup`](Self::set_markup) (D2). Toggling
    /// visibility changes the handle's natural width (a hidden GtkSpinner is
    /// skipped in `measure`), so `queue_resize` re-runs the strip layout —
    /// `GtkSpinner::start`/`stop` gate the frame-clock animation so a stopped,
    /// hidden spinner costs nothing.
    pub(super) fn set_busy(&self, content: &gtk::Widget, busy: bool) {
        let Some(idx) = self.index_of(content) else {
            return;
        };
        if let Some(t) = self.imp().tabs.borrow().get(idx) {
            if busy {
                t.spinner.set_visible(true);
                t.spinner.start();
            } else {
                t.spinner.stop();
                t.spinner.set_visible(false);
            }
        }
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
