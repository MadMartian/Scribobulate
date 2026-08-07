//! [`TabBar`]'s structural mutations — adding, removing, switching, and
//! drag-reordering tabs — plus the sibling-slide animation that eases handles
//! to their new resting slots. Every geometric decision (which slot a drop
//! reslots to, the active index after a remove, the per-frame ease) is the pure
//! [`super::layout`]; this file owns the GTK mutations and the frame-clock loop.

use super::layout;
use super::*;

/// Assumed frame delta (s) for the very first animation frame, before the frame
/// clock has a previous timestamp to difference against (~60fps).
const FALLBACK_FRAME_DT: f64 = 1.0 / 60.0;
/// Upper clamp (s) on a measured frame delta, so a long stall (tab hidden,
/// compositor frozen) can't make the ease jump the whole distance in one step.
const MAX_FRAME_DT: f64 = 0.1;

impl TabBar {
    // ── construction / teardown ─────────────────────────────────────────────

    pub(super) fn add_tab(&self, content: &gtk::Widget) {
        let label = gtk::Label::new(Some(""));
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.set_max_width_chars(24);
        label.set_xalign(0.0);

        // Leading busy indicator — hidden by default (a normal tab shows nothing),
        // revealed + spun only for a deferred tab awaiting its first render (see
        // `TabEntry::spinner`). Kept small so it doesn't dominate the handle.
        let spinner = gtk::Spinner::new();
        spinner.set_visible(false);
        spinner.set_valign(gtk::Align::Center);
        spinner.add_css_class("tab-spinner");

        // A GTK built-in icon name (always resolves — sidesteps the
        // icon-availability risk flagged in the retired tab-widget plan §E) revealed on hover via
        // CSS (preview.rs's `tabbar` rules), not always-on, to cut clutter.
        let close_btn = gtk::Button::from_icon_name(crate::icons::Icon::WindowClose.name());
        close_btn.add_css_class("flat");
        close_btn.add_css_class(TAB_CLOSE_BTN_CLASS);
        close_btn.set_valign(gtk::Align::Center);
        crate::a11y::name(&close_btn, "Close tab");

        let handle = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        handle.add_css_class("tab-handle");
        handle.set_margin_start(8);
        handle.set_margin_end(4);
        // VERTICAL margins are deliberately NOT set here — they live in CSS
        // (`preview::css`'s `tabbar .tab-handle` rules) because the active tab must
        // cancel its bottom margin to meet the strip's baseline rule. A widget
        // margin (`set_margin_bottom`) and a CSS `margin-bottom` are INDEPENDENT
        // and CUMULATIVE — GtkWidget applies its own in `gtk_widget_allocate`, the
        // CSS box applies its own inside that — so a 4px set here would have added
        // to, not been overridden by, the CSS rule, holding every tab 4px clear of
        // the baseline no matter what the stylesheet asked for. One axis, one
        // margin supplier. Horizontal margins stay in Rust: `layout` measures
        // handle widths for the strip's hit-test arithmetic.
        handle.append(&spinner);
        handle.append(&label);
        handle.append(&close_btn);
        // Parent each handle BEFORE the chevrons in child order (i.e. inserted
        // immediately before `prev_btn`, after any existing handles) so the two
        // chevrons stay the LAST children — hence topmost for input picking
        // (GTK picks children in reverse child order). Without this a handle
        // that has scrolled under a chevron is picked first and its close
        // button steals the chevron's click. Handle-vs-handle order is
        // preserved (new handle lands after existing ones), so drag-reorder
        // paint order is unchanged. Falls back to append if the chevrons aren't
        // built yet (they always are — created in `TabBar::new`).
        let prev = self.imp().prev_btn.borrow().clone();
        handle.insert_before(self, prev.as_ref());

        close_btn.connect_clicked(glib::clone!(
            #[weak(rename_to = bar)]
            self,
            #[strong]
            content,
            move |_| {
                let cb_ref = bar.imp().close_cb.borrow();
                if let Some(cb) = cb_ref.as_ref() {
                    cb(&content);
                }
            }
        ));

        self.imp().tabs.borrow_mut().push(imp::TabEntry {
            content: content.clone(),
            handle,
            label,
            spinner,
            current_x: Cell::new(0.0),
            target_x: Cell::new(0.0),
        });
        // A brand-new tab always appends at the end, which never shifts any
        // EXISTING tab's target_x — so there is nothing to animate; snap the
        // new handle straight to its resting slot instead of sliding it in.
        self.recompute_targets();
        if let Some(last) = self.imp().tabs.borrow().last() {
            last.current_x.set(last.target_x.get());
        }
        self.queue_allocate();
    }

    pub(super) fn remove_at(&self, idx: usize) -> Option<gtk::Widget> {
        let mut tabs = self.imp().tabs.borrow_mut();
        if idx >= tabs.len() {
            return None;
        }
        let entry = tabs.remove(idx);
        entry.handle.unparent();
        let remaining = tabs.len();
        drop(tabs);
        let was_active = self.imp().active_idx.get() == Some(idx);
        self.imp().active_idx.set(layout::active_after_remove(
            self.imp().active_idx.get(),
            idx,
        ));
        self.retarget_and_animate();
        // GtkNotebook auto-advanced to a neighboring page when the current
        // one was removed, firing `switch-page` — without this, closing the
        // active tab left the GtkStack showing nothing at all (bug found by
        // live Xvfb testing: outline/title stayed on the just-closed tab and
        // the content pane went blank). `layout::neighbor_after_remove` picks
        // the tab that slid into the closed one's slot (or the new last tab, if
        // the closed one was last) — the same neighbor GtkNotebook preferred.
        if was_active {
            if let Some(neighbor) = layout::neighbor_after_remove(idx, remaining) {
                self.switch_to_index(neighbor);
            }
        }
        Some(entry.content)
    }

    pub(super) fn remove_by_content(&self, content: &gtk::Widget) -> Option<usize> {
        let idx = self.index_of(content)?;
        self.remove_at(idx);
        Some(idx)
    }

    // ── switching ────────────────────────────────────────────────────────────

    pub(super) fn switch_to_index(&self, idx: usize) {
        let imp = self.imp();
        if imp.active_idx.get() == Some(idx) || idx >= imp.tabs.borrow().len() {
            return;
        }
        if let Some(prev) = imp.active_idx.get() {
            if let Some(t) = imp.tabs.borrow().get(prev) {
                t.handle.remove_css_class("active");
            }
        }
        let content = {
            let tabs = imp.tabs.borrow();
            let Some(t) = tabs.get(idx) else { return };
            t.handle.add_css_class("active");
            t.content.clone()
        };
        imp.active_idx.set(Some(idx));
        self.scroll_into_view(idx);
        // Internal (stack-visible-child sync, `TabView::new`) first, then the
        // externally-registered one (`TabView::connect_switch_page`) — see
        // the doc comment on `imp::TabBar::internal_switch_cb`.
        if let Some(cb) = imp.internal_switch_cb.borrow().as_ref() {
            cb(&content, idx as u32);
        }
        if let Some(cb) = imp.switch_cb.borrow().as_ref() {
            cb(&content, idx as u32);
        }
    }

    /// Mark index 0 active WITHOUT firing any switch callback — for a window's
    /// initial tab, which `GtkStack` shows by default but which never travels
    /// through `switch_to_index` (its per-tab state is set up directly by
    /// `build_window`, not via `on_active_tab_changed`). Without this, `active_idx`
    /// stays `None` for a never-switched first tab, which silently breaks anything
    /// keyed on it: `remove_at`'s `was_active` check (so moving/closing that first
    /// tab left the source window blank with the removed tab's stale outline) and
    /// `current_page` (so Next/Previous Tab no-op'd from it). Idempotent — a no-op
    /// once any tab has been switched to.
    pub(super) fn mark_first_active(&self) {
        let imp = self.imp();
        if imp.active_idx.get().is_some() {
            return;
        }
        if let Some(t) = imp.tabs.borrow().first() {
            t.handle.add_css_class("active");
        }
        if !imp.tabs.borrow().is_empty() {
            imp.active_idx.set(Some(0));
        }
    }

    // ── reorder / cross-window drag hooks (wired externally, `window/tabs/`'s
    // `wire_tab_bar_dnd` — see module doc, simplification 1) ──────────────────

    /// Reslot `dragged` (if it is one of THIS bar's own tabs — a no-op
    /// otherwise, so a foreign drag merely hovering over this bar before
    /// crossing elsewhere does nothing) to the index implied by `hover_x`. The
    /// index itself is the pure [`layout::reorder_index`]; this method owns the
    /// `Vec` splice and the active-index re-derivation across the move.
    pub(super) fn preview_reorder(&self, dragged: &gtk::Widget, hover_x: f64) {
        let imp = self.imp();
        let off = self.scroll_offset();
        let mut tabs = imp.tabs.borrow_mut();
        let Some(from) = tabs
            .iter()
            .position(|t| t.content.as_ptr() == dragged.as_ptr())
        else {
            return;
        };
        // `hover_x` is bar-local (same space `index_at` receives) — convert
        // to logical/content space the same way.
        let logical_x = (hover_x - imp.tabs_x0.get()) + off;
        let spans: Vec<layout::Span> = tabs
            .iter()
            .map(|t| layout::Span {
                start: t.target_x.get(),
                width: natural_width(&t.handle),
            })
            .collect();
        let new_index = layout::reorder_index(logical_x, from, &spans);
        if new_index != from {
            // `active_idx` is a raw index into `tabs` — moving an entry
            // shifts every OTHER tab between `from` and the insertion point
            // by one, silently invalidating it (bug found by live Xvfb
            // testing: right-clicking a just-reordered, non-active tab and
            // choosing "Move to New Window" moved the wrong one, because
            // `active_idx` still pointed at whatever tab happened to occupy
            // its old numeric slot — the CSS `.active` class stayed correct,
            // since it's attached to the widget itself, which is exactly why
            // this was invisible on screen). Re-derive it from the actual
            // active tab's identity across the move rather than leaving it
            // untouched.
            let active_content = imp
                .active_idx
                .get()
                .and_then(|i| tabs.get(i))
                .map(|t| t.content.clone());
            let entry = tabs.remove(from);
            let clamped = new_index.min(tabs.len());
            tabs.insert(clamped, entry);
            if let Some(active_content) = active_content {
                imp.active_idx.set(
                    tabs.iter()
                        .position(|t| t.content.as_ptr() == active_content.as_ptr()),
                );
            }
        }
        drop(tabs);
        self.retarget_and_animate();
    }

    /// Re-assert final resting positions once a drag settles (drop or
    /// drag-end) — idempotent; `preview_reorder` already left the strip in
    /// its final order, this just ensures the animation is running to reach
    /// it if a caller races `drag-end` ahead of the last `motion`.
    pub(super) fn settle_reorder(&self) {
        self.retarget_and_animate();
    }

    // ── animation (the retired tab-widget plan §A, ported near-verbatim) ────────────
    // Scroll position is no longer part of this — it's a real `GtkAdjustment`
    // now (`scroll_by`/`scroll_into_view` below), and `GtkAdjustment` has no
    // built-in easing; deliberately left un-eased (an instant `set_value`)
    // since a hand-rolled multi-frame ease was the residual-warning's own
    // root cause (GTK4Rs/AP-104's addendum) — the tick loop here is
    // purely the tab-handle sibling-slide reorder animation now.

    pub(super) fn recompute_targets(&self) {
        let tabs = self.imp().tabs.borrow();
        let mut x = 0.0;
        for tab in tabs.iter() {
            let w = natural_width(&tab.handle);
            tab.target_x.set(x);
            x += w + TAB_SPACING;
        }
    }

    pub(super) fn retarget_and_animate(&self) {
        self.recompute_targets();
        self.ensure_tick();
    }

    pub(super) fn scroll_by(&self, delta: f64) {
        let imp = self.imp();
        let Some(adj) = imp.hadjustment.borrow().clone() else {
            return;
        };
        let max = (adj.upper() - adj.page_size()).max(0.0);
        crate::saferizer::scrollpos::jump(&adj, (adj.value() + delta).clamp(0.0, max));
    }

    pub(super) fn scroll_into_view(&self, idx: usize) {
        let imp = self.imp();
        let (pos, w) = {
            let tabs = imp.tabs.borrow();
            let Some(t) = tabs.get(idx) else { return };
            (t.target_x.get(), natural_width(&t.handle))
        };
        let Some(adj) = imp.hadjustment.borrow().clone() else {
            return;
        };
        // The viewport available to tab handles (post-gutter-reservation),
        // as of the last `size_allocate` — NOT the adjustment's own
        // `page_size`, which can be one frame stale relative to a chevron
        // visibility change that hasn't reallocated yet. The reveal decision
        // itself is the pure [`layout::scroll_target`].
        let viewport = imp.viewport_w.get();
        let value = adj.value();
        if let Some(new_value) = layout::scroll_target(pos, w, value, viewport) {
            crate::saferizer::scrollpos::jump(&adj, new_value);
        }
    }

    pub(super) fn ensure_tick(&self) {
        if self.imp().tick_id.borrow().is_some() {
            self.queue_allocate();
            return;
        }
        let id = self.add_tick_callback(|bar, clock| bar.animate_tick(clock));
        *self.imp().tick_id.borrow_mut() = Some(id);
    }

    pub(super) fn animate_tick(&self, clock: &gdk::FrameClock) -> glib::ControlFlow {
        let imp = self.imp();
        let now = clock.frame_time();
        let last = imp.last_frame.get();
        let dt = if last == 0 {
            FALLBACK_FRAME_DT
        } else {
            ((now - last) as f64 / 1_000_000.0).clamp(0.0, MAX_FRAME_DT)
        };
        imp.last_frame.set(now);
        let k = layout::ease_factor(dt, EASE_TAU);

        let mut settled = true;
        for tab in imp.tabs.borrow().iter() {
            let nc = layout::ease_step(tab.current_x.get(), tab.target_x.get(), k);
            tab.current_x.set(nc);
            if !layout::is_settled(nc, tab.target_x.get()) {
                settled = false;
            }
        }

        // Animation is position-only — `queue_allocate`, never `queue_resize`
        // (the retired tab-widget plan gotcha #4: a per-frame `queue_resize` re-measures
        // everything and can loop).
        self.queue_allocate();

        if settled {
            for tab in imp.tabs.borrow().iter() {
                tab.current_x.set(tab.target_x.get());
            }
            imp.last_frame.set(0);
            *imp.tick_id.borrow_mut() = None;
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    }
}
