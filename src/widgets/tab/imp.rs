//! The `GObject` subclass backing [`super::TabBar`] — the strip's imp struct,
//! its `GtkScrollable` property overrides, and its `measure`/`size_allocate`/
//! `snapshot` layout vfuncs. The load-bearing arithmetic these vfuncs drive
//! lives in the pure [`super::layout`] module; this file owns only the GObject
//! glue and the `measure()` calls that feed it.

use super::layout;
use super::*;

pub(super) struct TabEntry {
    pub(super) content: gtk::Widget,
    pub(super) handle: gtk::Box,
    pub(super) label: gtk::Label,
    /// Small leading busy indicator, shown (and spinning) only while this tab is
    /// a not-yet-rendered DEFERRED tab (`TabState::needs_render`) — a
    /// background-warmed multi-file `open` / session-restore tab whose preview
    /// hasn't been built yet. Toggled via [`super::TabBar::set_busy`] from the
    /// tab lifecycle: on it becomes visible when the deferred tab is created and
    /// hidden when the tab materializes. Hidden (and stopped) it takes zero width
    /// — GtkBox skips an invisible child in `measure` — so a settled tab strip is
    /// byte-identical to before this existed.
    pub(super) spinner: gtk::Spinner,
    pub(super) current_x: Cell<f64>,
    pub(super) target_x: Cell<f64>,
}

type SwitchCb = Box<dyn Fn(&gtk::Widget, u32)>;
type CloseCb = Box<dyn Fn(&gtk::Widget)>;
type MenuCb = Box<dyn Fn(&gtk::Widget, f64, f64)>;

#[derive(Default)]
pub(super) struct TabBar {
    pub(super) tabs: RefCell<Vec<TabEntry>>,
    pub(super) prev_btn: RefCell<Option<gtk::Button>>,
    pub(super) next_btn: RefCell<Option<gtk::Button>>,
    pub(super) tick_id: RefCell<Option<gtk::TickCallbackId>>,
    pub(super) last_frame: Cell<i64>,
    /// The `GtkScrollable` interface's own adjustments — `hadjustment` is
    /// the real source of truth for horizontal scroll position
    /// (self-created and self-assigned in `TabBar::new`, NOT supplied by
    /// a wrapping `GtkScrolledWindow` — see the module doc); `vadjustment`
    /// is stored only because the interface requires the property to
    /// exist, never read (`TabBar` never scrolls vertically).
    pub(super) hadjustment: RefCell<Option<gtk::Adjustment>>,
    pub(super) vadjustment: RefCell<Option<gtk::Adjustment>>,
    pub(super) hadjustment_handler: RefCell<Option<glib::SignalHandlerId>>,
    /// Guards `hadjustment`'s `configure()` call in `size_allocate`
    /// against its own re-entrancy: `configure` can synchronously emit
    /// `value-changed` (e.g. when clamping the value to a new, smaller
    /// `upper`), which our own handler responds to with
    /// `queue_allocate()` — undesirable, and unnecessary, while already
    /// inside the very `size_allocate` pass that call would just repeat
    /// (the retired tab-widget plan §G gotcha #10, the same guard Adw's own
    /// `AdwTabBox` uses around its analogous `configure` call).
    pub(super) block_scrolling: Cell<bool>,
    pub(super) content_width: Cell<f64>,
    /// Width available to the tab handles themselves (allocated width
    /// minus whatever gutter is currently reserved for the chevrons) and
    /// the x-offset the handles are laid out FROM (the left gutter's
    /// width, reserved or not) — both recomputed every `size_allocate`
    /// and consulted by `scroll_into_view`/hit-testing so the bar-local
    /// pixel space they work in always matches what was actually drawn.
    pub(super) viewport_w: Cell<f64>,
    pub(super) tabs_x0: Cell<f64>,
    /// Last `show_prev`/`show_next` actually applied — `size_allocate`
    /// can run several times per interaction, and re-touching
    /// `set_sensitive` every single time even when the value hasn't
    /// changed is pointless churn.
    pub(super) prev_shown: Cell<bool>,
    pub(super) next_shown: Cell<bool>,
    pub(super) active_idx: Cell<Option<usize>>,
    // Two separate slots, both invoked by `switch_to_index` (bug found by
    // live Xvfb testing, the retired tab-widget plan implementation): a single
    // shared slot cannot serve both `TabView::new`'s own internal
    // stack-visible-child sync (installed once, unconditionally) and
    // `TabView::connect_switch_page`'s externally-registered callback
    // (`window/tabs/`'s `wire_tab_switch_page`) without the external
    // `connect_*` call silently clobbering the internal sync.
    pub(super) internal_switch_cb: RefCell<Option<SwitchCb>>,
    pub(super) switch_cb: RefCell<Option<SwitchCb>>,
    pub(super) close_cb: RefCell<Option<CloseCb>>,
    pub(super) menu_cb: RefCell<Option<MenuCb>>,
    pub(super) page_added_cb: RefCell<Option<SwitchCb>>,
}

#[glib::object_subclass]
impl ObjectSubclass for TabBar {
    const NAME: &'static str = "ScribobulateTabBar";
    type Type = super::TabBar;
    type ParentType = gtk::Widget;
    type Interfaces = (gtk::Scrollable,);

    fn class_init(klass: &mut Self::Class) {
        // GTK-4.6 gotcha #5 (the retired tab-widget plan): `set_css_name` only in
        // `class_init`, never per-instance. Matches the retargeted
        // `tabbar:drop(active)` CSS rule in `preview.rs`.
        klass.set_css_name("tabbar");
    }
}

impl ObjectImpl for TabBar {
    // The four `GtkScrollable` interface properties, declared as
    // OVERRIDES (not fresh `ParamSpec`s — the retired tab-widget plan §G) of the
    // interface's own. `hscroll-policy = Minimum` (§G) since the strip's
    // own natural width is unrelated to how much of it currently fits.
    fn properties() -> &'static [glib::ParamSpec] {
        static PROPS: std::sync::OnceLock<Vec<glib::ParamSpec>> = std::sync::OnceLock::new();
        PROPS.get_or_init(|| {
            vec![
                glib::ParamSpecOverride::for_interface::<gtk::Scrollable>("hadjustment"),
                glib::ParamSpecOverride::for_interface::<gtk::Scrollable>("vadjustment"),
                glib::ParamSpecOverride::for_interface::<gtk::Scrollable>("hscroll-policy"),
                glib::ParamSpecOverride::for_interface::<gtk::Scrollable>("vscroll-policy"),
            ]
        })
    }

    fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
        match pspec.name() {
            "hadjustment" => {
                let new_adj: Option<gtk::Adjustment> = value.get().ok();
                if let Some(old) = self.hadjustment.borrow().as_ref() {
                    if let Some(id) = self.hadjustment_handler.borrow_mut().take() {
                        old.disconnect(id);
                    }
                }
                if let Some(adj) = &new_adj {
                    let obj = self.obj();
                    let id = adj.connect_value_changed(glib::clone!(
                        #[weak(rename_to = bar)]
                        obj,
                        move |_| {
                            // See `block_scrolling`'s doc comment: skip the
                            // reallocate this handler would otherwise
                            // trigger while `size_allocate` itself is the
                            // one synchronously adjusting the value (via
                            // `configure`) — it's already mid-layout.
                            if !bar.imp().block_scrolling.get() {
                                bar.queue_allocate();
                            }
                        }
                    ));
                    *self.hadjustment_handler.borrow_mut() = Some(id);
                }
                *self.hadjustment.borrow_mut() = new_adj;
            }
            "vadjustment" => {
                *self.vadjustment.borrow_mut() = value.get().ok();
            }
            // Both policies are fixed (Minimum/Natural) and reported
            // directly from `property()` below — nothing to store.
            "hscroll-policy" | "vscroll-policy" => {}
            _ => unimplemented!(),
        }
    }

    fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
        match pspec.name() {
            "hadjustment" => self.hadjustment.borrow().to_value(),
            "vadjustment" => self.vadjustment.borrow().to_value(),
            "hscroll-policy" => gtk::ScrollablePolicy::Minimum.to_value(),
            "vscroll-policy" => gtk::ScrollablePolicy::Natural.to_value(),
            _ => unimplemented!(),
        }
    }

    // GTK-4.6 gotcha #1/#2: unparent every child (the handles AND the
    // chevrons — GTK does NOT do this automatically for a custom widget)
    // and remove the tick callback before it can fire against a
    // half-finalized widget.
    fn dispose(&self) {
        if let Some(id) = self.tick_id.borrow_mut().take() {
            id.remove();
        }
        // The H2 leak is fixed at closure FORMATION: every `TabView::connect_*`
        // façade closure stored in the imp callback cells now weak-captures the
        // `TabView` (see `WeakTabView`), so no cell forms a `bar → cell → tv →
        // bar` cycle. Do NOT be tempted to "also break the cycle here" — in
        // GTK4 `dispose` runs at *finalize* (refcount 0), which a cycle would
        // prevent, so clearing the cells here could never rescue a strong
        // capture (that trap is ScrAP-60). The cells drop with the imp
        // struct at finalize; nothing to sever.
        crate::widgets::unparent_all_children(&*self.obj());
    }
}

impl WidgetImpl for TabBar {
    // Neither axis depends on the other's `for_size` — matches
    // `ScribTableWidget`'s exact pattern (widgets/table).
    fn request_mode(&self) -> gtk::SizeRequestMode {
        gtk::SizeRequestMode::ConstantSize
    }

    // Horizontal is deliberately (0, 0): the strip must never inflate the
    // window's own natural width by the number of open tabs. The real
    // content width is reported to `hadjustment.upper` in
    // `size_allocate` instead — measure/allocate stay decoupled from it,
    // exactly as a `GtkScrollable` is meant to (its own size is driven by
    // the viewport it's given, not by its scrollable content).
    fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
        match orientation {
            gtk::Orientation::Horizontal => (0, 0, -1, -1),
            _ => {
                let mut h = self
                    .tabs
                    .borrow()
                    .iter()
                    .map(|t| t.handle.measure(gtk::Orientation::Vertical, -1).1)
                    .max()
                    .unwrap_or(0);
                for btn in [self.prev_btn.borrow(), self.next_btn.borrow()] {
                    if let Some(btn) = btn.as_ref() {
                        h = h.max(btn.measure(gtk::Orientation::Vertical, -1).1);
                    }
                }
                (h, h, -1, -1)
            }
        }
    }

    fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
        let (Some(prev_btn), Some(next_btn)) = (
            self.prev_btn.borrow().clone(),
            self.next_btn.borrow().clone(),
        ) else {
            return;
        };
        let tabs = self.tabs.borrow();

        // Full-width content extent (as if the chevrons reserved no
        // space at all) decides whether the strip overflows in the first
        // place — see the module doc. `layout::resolve_gutter` turns it,
        // the width, and the chevrons' true natural width into the gutter
        // reservation and the resulting tab viewport, self-consistently.
        let full_content_w = tabs
            .last()
            .map(|t| t.target_x.get() + natural_width(&t.handle))
            .unwrap_or(0.0);
        self.content_width.set(full_content_w);

        // The chevrons' TRUE natural width — computed unconditionally
        // and NEVER shrunk. A shown chevron is always allocated exactly
        // this, matching what `GtkButton`'s own internal layout expects;
        // giving it anything smaller (down to and including 0) subtracts
        // its CSS padding/border without clamping at 0 internally,
        // driving its child `GtkImage`'s computed width negative
        // (ScrAP-56).
        let chevron_w = prev_btn
            .measure(gtk::Orientation::Horizontal, -1)
            .1
            .max(next_btn.measure(gtk::Orientation::Horizontal, -1).1);
        let layout::Gutter {
            reserved,
            viewport_w,
        } = layout::resolve_gutter(width, full_content_w, chevron_w);
        self.viewport_w.set(viewport_w as f64);
        self.tabs_x0.set(reserved as f64);

        // Configure the real `GtkAdjustment` (the retired tab-widget plan §G):
        // `lower` 0, `upper` the full logical content width (never less than
        // the viewport), `page_size` the viewport, step/page increments a
        // fraction of it. Guarded against `configure`'s own re-entrant
        // `value-changed` (see `block_scrolling`'s doc comment).
        let (upper, max_off) = layout::scroll_extent(full_content_w, viewport_w as f64);
        let (step_inc, page_inc) = layout::page_steps(viewport_w as f64);
        // `configure()` emits `::changed`/`notify`/`value-changed`
        // SYNCHRONOUSLY, in-line, before returning (confirmed against
        // GTK 4.6.9 source, `gtkadjustment.c:857-861`) — so our own
        // `value-changed` handler (installed in `set_property`) runs
        // to completion BEFORE `configure()` below returns, while this
        // very function is still on the stack. `block_scrolling` stops
        // that handler from re-entering `queue_allocate`, but a `Cell`
        // guard does NOT stop a `RefCell` double-borrow: `.borrow()`
        // must be released before the call, not held across it, or any
        // code the synchronous re-entry reaches that also touches this
        // `RefCell` panics with `BorrowMutError` (silent abort — no
        // panic message, since it happens inside a GObject C callback
        // trampoline). Clone the `Adjustment` out (cheap — a GObject
        // refcount bump) and let the borrow end before calling
        // `configure` (researcher-confirmed root cause of a real,
        // live-reproduced crash during this file's `GtkScrollable`
        // rewrite — the same pattern as ScrAP-53).
        let hadj = self.hadjustment.borrow().clone();
        let off = if let Some(adj) = hadj {
            self.block_scrolling.set(true);
            let value = adj.value().clamp(0.0, max_off);
            adj.configure(value, 0.0, upper, step_inc, page_inc, viewport_w as f64);
            self.block_scrolling.set(false);
            adj.value()
        } else {
            0.0
        };

        let (show_prev, show_next) = layout::chevron_visibility(reserved, off, max_off);

        // Neither chevron's `:visible` is EVER toggled — "absent" is
        // achieved purely by moving the unneeded one outside
        // `[0, width)`, where the constructor's `set_overflow(Hidden)`
        // clips it from the paint exactly the way a scrolled-off tab
        // handle already is (ScrAP-56's full account of why).
        // `set_sensitive` (layout-neutral) still keeps a shoved-off
        // chevron unfocusable/unclickable — only called when the value
        // actually changes, to keep this per-frame-safe.
        if self.prev_shown.replace(show_prev) != show_prev {
            prev_btn.set_sensitive(show_prev);
        }
        let prev_x = if show_prev {
            0
        } else {
            -(chevron_w + layout::OFFSCREEN_MARGIN)
        };
        prev_btn.size_allocate(&gdk::Rectangle::new(prev_x, 0, chevron_w, height), baseline);

        for tab in tabs.iter() {
            let w = tab.handle.measure(gtk::Orientation::Horizontal, -1).1;
            let x = reserved + (tab.current_x.get() - off).round() as i32;
            tab.handle
                .size_allocate(&gdk::Rectangle::new(x, 0, w, height), baseline);
        }

        if self.next_shown.replace(show_next) != show_next {
            next_btn.set_sensitive(show_next);
        }
        let next_x = if show_next {
            width - chevron_w
        } else {
            width + layout::OFFSCREEN_MARGIN
        };
        next_btn.size_allocate(&gdk::Rectangle::new(next_x, 0, chevron_w, height), baseline);
    }

    // Paint the tab handles CLIPPED to the scrollable viewport, then the
    // chevrons on top, unclipped. `size_allocate` gives each handle its
    // full natural width wherever it scrolls to, so a partially-scrolled
    // handle's rectangle extends into (and, being a later child, would
    // paint over) the chevron gutter `[width-chevron_w, width)`; the bar's
    // own `Overflow::Hidden` only clips at the outer edge, not at the
    // gutter boundary, so without this the rightmost handle's label/close
    // glyphs bled through the flat (transparent) chevron — the "mixed-up
    // rendering" the operator saw. Clipping the handles to
    // `[tabs_x0, tabs_x0+viewport_w)` confines them to the viewport; the
    // chevrons then own their gutters cleanly. (Pick order is fixed
    // separately — `add_tab` parents handles BEFORE the chevrons so the
    // chevrons are topmost for input, and `index_at` ignores gutter x — so
    // a bleeding handle's close button can no longer steal a chevron click.)
    // NB: the widget's OWN css background/`:drop(active)` box-shadow is
    // rendered by GTK's snapshot wrapper around this vfunc, not here, so
    // overriding snapshot does not drop it.
    fn snapshot(&self, snapshot: &gtk::Snapshot) {
        let obj = self.obj();
        let h = obj.height() as f32;
        let x0 = self.tabs_x0.get() as f32;
        let vp_w = self.viewport_w.get() as f32;
        // Before the first real allocation both are 0; fall back to the
        // full width so the strip is never blank for that one frame.
        let vp_w = if vp_w > 0.0 { vp_w } else { obj.width() as f32 };
        snapshot.push_clip(&gtk::graphene::Rect::new(x0, 0.0, vp_w, h));
        for tab in self.tabs.borrow().iter() {
            obj.snapshot_child(&tab.handle, snapshot);
        }
        snapshot.pop();
        if let Some(btn) = self.prev_btn.borrow().as_ref() {
            obj.snapshot_child(btn, snapshot);
        }
        if let Some(btn) = self.next_btn.borrow().as_ref() {
            obj.snapshot_child(btn, snapshot);
        }
    }
}

impl ScrollableImpl for TabBar {}
