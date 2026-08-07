//! `SplitView` — a per-tab, orderable two-pane splitter that mounts the reused
//! `GtkSourceView` editor **once and never reparents it**: the structural fix for
//! the use-after-free (the transferable lesson is ScrAP-58; this module is that
//! fix's living record).
//!
//! ## Why this exists (the reparent UAF)
//!
//! The document editor is a single `GtkSourceView`, reused across mode switches
//! (D6 — preserves undo/cursor). Its line-number gutter installs a
//! `G_BINDING_SYNC_CREATE` binding `view."vadjustment" → GtkSourceGutter`'s
//! internal signal group and **never unbinds it** (upstream defect,
//! `gtksourcegutter.c`, present in 5.4.1 and `main`). The *old* design rebuilt
//! the mode container on every switch, so the reused view was **reparented**
//! (fresh `GtkScrolledWindow` for edit, a `GtkPaned` for split). Each reparent
//! re-ran `gtk_scrolled_window_set_child` → `notify::vadjustment`, re-firing that
//! binding; in the same cascade a `gtk_widget_remove_controller` freed a block
//! that the binding's `set_property` → `g_object_unref` then read — a confirmed,
//! valgrind-verified use-after-free (six `g_object_unref: 'G_IS_OBJECT' failed`
//! per switch on the 2nd+ switch; UB, only masked today by `g_return_if_fail`).
//!
//! `SplitView` closes it *by construction*: the editor's `GtkScrolledWindow` is
//! `set_parent`ed **once** in [`SplitView::new`] and its child slot is NEVER
//! reassigned again. Mode (Preview/Edit/Split), split orientation (H/V) and split
//! order (swap) are pure **layout parameters** applied in `measure`/`size_allocate`
//! — a hidden pane is `set_child_visible(false)` (unmapped, not reparented), a
//! swap flips allocation order, an orientation change flips the split axis. None
//! of these calls `gtk_scrolled_window_set_child`, so `notify::vadjustment` never
//! re-fires and the gutter binding never re-runs → no reparent, no UAF, in *every*
//! combination including vertical-swap (which a plain `GtkPaned` could only do by
//! reparenting — the residual the custom widget exists to avoid; see the plan's
//! probe/spike table).
//!
//! ## Layout
//!
//! Three children, all mounted once (mirrors `TabBar`/`ScribTableWidget`'s
//! subclassing idiom):
//! - `editor_scroller`: `GtkScrolledWindow` wrapping the reused `GtkSourceView`.
//!   **Load-bearing:** never reparented after construction.
//! - `preview_holder`: a `GtkBox` whose single child is the current preview
//!   `GtkScrolledWindow`. The preview is a `CodePreviewView` (no gutter binding)
//!   and is freely rebuilt/freed on mode switches / re-renders via
//!   [`SplitView::set_preview`] — only the *editor* carries the offending binding,
//!   so only the editor must be pinned (the plan's "load-bearing narrowing").
//! - `divider`: a `GtkSeparator` shown only in split mode, dragged to move the
//!   split position (a capture-free `GtkGestureDrag` on the SplitView itself — see
//!   `new` — so the divider's own motion can't drift the drag offset the way a
//!   gesture attached to the moving handle would).
//!
//! `content_box` (the tab body / stack page) holds exactly one child: this
//! `SplitView`, for the tab's whole lifetime. Every former
//! `content_box.first_child().downcast::<Paned/ScrolledWindow>()` call site now
//! reaches the panes through [`SplitView::editor_scroller`] /
//! [`SplitView::preview_scroller`] instead.

use super::*;
use gtk::subclass::prelude::*;
use std::cell::OnceCell;

/// The minimum size of `w` along `orientation` — names `measure`'s 4-tuple so no
/// call site reaches for a positional `.0` (QA L-8).
fn measure_min(w: &impl IsA<gtk::Widget>, orientation: gtk::Orientation) -> i32 {
    let (min, _nat, _, _) = w.measure(orientation, -1);
    min
}

/// Thickness of the draggable divider gutter (px), and the pointer hit-slop
/// added on each side of it so the resize grab isn't a pixel-perfect target.
const HANDLE_SIZE: f64 = 8.0;
const HANDLE_SLOP: f64 = 3.0;
/// Minimum pixels each pane is kept to along the split axis, on top of each
/// pane's own reported minimum — so a drag can't collapse a pane to nothing.
const MIN_PANE: f64 = 60.0;

mod imp {
    use super::*;

    #[derive(Default)]
    pub(crate) struct SplitView {
        pub(super) editor_scroller: OnceCell<gtk::ScrolledWindow>,
        /// The `GtkOverlay` that wraps `editor_scroller` — the actual DIRECT child of
        /// the SplitView for the editor pane (measure/allocate/snapshot/ordered_panes
        /// target this), so the editor's in-surface Annotate card can float over the
        /// scroller (GTK4Rs/AP-83, mirroring the preview pane). The scroller inside it is
        /// still `set_child`ed exactly once and never reparented — the reparent
        /// invariant holds (the card is an *added overlay*, never the main child).
        pub(super) editor_overlay: OnceCell<gtk::Overlay>,
        /// Raises the editor pane's in-surface Annotate card over the current editor
        /// selection. Wired once in `new` onto `editor_overlay`;
        /// the `win.annotate` action calls it via [`super::SplitView::trigger_editor_annotate`]
        /// when the editor is the active pane. Captures only weak refs (no cycle, ScrAP-60).
        pub(super) editor_annotate_trigger: RefCell<Option<Rc<dyn Fn()>>>,
        pub(super) preview_holder: OnceCell<gtk::Box>,
        pub(super) divider: OnceCell<gtk::Separator>,
        /// Current mode — `Cell<ViewMode>`'s `Default` is `Preview` (matching
        /// every tab's start state), so a freshly-constructed `SplitView` is
        /// already in the right initial mode before `set_layout` is first called.
        pub(super) mode: Cell<ViewMode>,
        pub(super) vertical: Cell<bool>,
        pub(super) swapped: Cell<bool>,
        /// Fraction of the split axis (minus the handle) given to the physically
        /// *first* pane (start/top). A fraction — not GtkPaned's absolute px — so
        /// the split ratio survives window resizes and orientation flips.
        pub(super) fraction: Cell<f64>,
        pub(super) dragging: Cell<bool>,
        /// The first pane's length at drag start; the live length during a drag
        /// is this plus the gesture's along-axis offset (drift-free because the
        /// gesture is on the non-moving `SplitView`, not the divider).
        pub(super) drag_anchor: Cell<f64>,
        /// Armed by [`super::SplitView::set_preview`] on every fresh preview mount;
        /// consumed by the first split-mode live re-render after it (the blank-overlay belt —
        /// see `scrollsync::arm_first_content_repaint`). A preview mounted EMPTY (a
        /// brand-new doc) then filled via `set_buffer` WHILE ALREADY VISIBLE takes its
        /// first REAL content-validation on an on-screen pane, which can leave the
        /// overlay terminally blank; the first render after a mount forces one healing
        /// follow-up frame. Steady-state edits never re-arm it.
        pub(super) preview_first_render_pending: Cell<bool>,
    }

    impl SplitView {
        pub(super) fn ed(&self) -> gtk::ScrolledWindow {
            self.editor_scroller.get().unwrap().clone()
        }
        /// The editor pane's DIRECT child of the SplitView — the overlay wrapping the
        /// scroller. Used everywhere the SplitView treats the editor as its own child
        /// (measure/size_allocate/snapshot/ordered_panes); `ed()` remains the scroller
        /// for scroll-sync/gutter/buffer access, which don't care about the nesting.
        pub(super) fn edpane(&self) -> gtk::Overlay {
            self.editor_overlay.get().unwrap().clone()
        }
        pub(super) fn holder(&self) -> gtk::Box {
            self.preview_holder.get().unwrap().clone()
        }
        pub(super) fn div(&self) -> gtk::Separator {
            self.divider.get().unwrap().clone()
        }
        /// The split axis's orientation for the current `vertical` flag: a
        /// vertical split stacks panes top/bottom (axis = Vertical); a horizontal
        /// split places them left/right (axis = Horizontal).
        fn split_axis(&self) -> gtk::Orientation {
            if self.vertical.get() {
                gtk::Orientation::Vertical
            } else {
                gtk::Orientation::Horizontal
            }
        }

        /// The two panes in physical order for the current `swapped` flag.
        /// Canonical (non-swapped): editor = start, preview = end (matches the
        /// old `GtkPaned` code's "editor=start" convention); swapped flips them.
        fn ordered_panes(&self) -> (gtk::Widget, gtk::Widget) {
            let ed: gtk::Widget = self.edpane().upcast();
            let pv: gtk::Widget = self.holder().upcast();
            if self.swapped.get() {
                (pv, ed)
            } else {
                (ed, pv)
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SplitView {
        const NAME: &'static str = "ScribobulateSplitView";
        type Type = super::SplitView;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for SplitView {
        // GTK does NOT unparent a custom widget's children automatically — do it
        // here, matching `TabBar::dispose`.
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for SplitView {
        // Both scrollers are `ConstantSize` (a `GtkScrolledWindow` scrolls rather
        // than growing height-for-width), so neither axis depends on the other's
        // `for_size` — same as `TabBar`.
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::ConstantSize
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            match self.mode.get() {
                // Single-pane modes: the SplitView measures exactly as the one
                // visible pane (the other pane and the divider are child-invisible
                // and contribute nothing).
                ViewMode::Edit => self.edpane().measure(orientation, for_size),
                ViewMode::Preview => self.holder().measure(orientation, for_size),
                ViewMode::Split => {
                    let (first, second) = self.ordered_panes();
                    let (fmin, fnat, _, _) = first.measure(orientation, -1);
                    let (smin, snat, _, _) = second.measure(orientation, -1);
                    if orientation == self.split_axis() {
                        // Along the split axis the two panes plus the handle
                        // stack, so their sizes add.
                        let handle = HANDLE_SIZE as i32;
                        (fmin + smin + handle, fnat + snat + handle, -1, -1)
                    } else {
                        // Across the split axis the panes overlap in extent, so
                        // the widget needs the larger of the two.
                        (fmin.max(smin), fnat.max(snat), -1, -1)
                    }
                }
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            let full = gdk::Rectangle::new(0, 0, width, height);
            match self.mode.get() {
                ViewMode::Edit => self.edpane().size_allocate(&full, baseline),
                ViewMode::Preview => self.holder().size_allocate(&full, baseline),
                ViewMode::Split => {
                    let vertical = self.vertical.get();
                    let axis = self.split_axis();
                    let (first, second) = self.ordered_panes();

                    let axis_len = if vertical { height } else { width } as f64;
                    let avail = (axis_len - HANDLE_SIZE).max(0.0);

                    // Each pane's minimum along the split axis, floored at
                    // MIN_PANE, so a drag can never collapse either pane.
                    let fmin = (measure_min(&first, axis) as f64).max(MIN_PANE);
                    let smin = (measure_min(&second, axis) as f64).max(MIN_PANE);

                    // Clamp the fraction-derived first length between its own
                    // minimum and whatever the second pane's minimum leaves —
                    // guarding the (degenerate, too-narrow) case where the two
                    // minima can't both be honoured (lo may exceed hi) by
                    // preferring the second pane's claim.
                    let hi = (avail - smin).max(0.0);
                    let lo = fmin.min(hi);
                    let first_len = (avail * self.fraction.get()).clamp(lo, hi.max(lo));
                    let first_len_i = first_len.round() as i32;
                    let handle_i = HANDLE_SIZE.round() as i32;

                    let (frect, drect, srect) = if vertical {
                        let second_h = (height - first_len_i - handle_i).max(0);
                        (
                            gdk::Rectangle::new(0, 0, width, first_len_i),
                            gdk::Rectangle::new(0, first_len_i, width, handle_i),
                            gdk::Rectangle::new(0, first_len_i + handle_i, width, second_h),
                        )
                    } else {
                        let second_w = (width - first_len_i - handle_i).max(0);
                        (
                            gdk::Rectangle::new(0, 0, first_len_i, height),
                            gdk::Rectangle::new(first_len_i, 0, handle_i, height),
                            gdk::Rectangle::new(first_len_i + handle_i, 0, second_w, height),
                        )
                    };
                    first.size_allocate(&frect, baseline);
                    self.div().size_allocate(&drect, baseline);
                    second.size_allocate(&srect, baseline);
                }
            }
        }

        // Snapshot only the children that were actually allocated for the current
        // mode — snapshotting a child-invisible, unallocated pane would trip the
        // "Trying to snapshot … without a current allocation" warning (GTK4Rs/AP-104).
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            match self.mode.get() {
                ViewMode::Edit => obj.snapshot_child(&self.edpane(), snapshot),
                ViewMode::Preview => obj.snapshot_child(&self.holder(), snapshot),
                ViewMode::Split => {
                    obj.snapshot_child(&self.edpane(), snapshot);
                    obj.snapshot_child(&self.holder(), snapshot);
                    obj.snapshot_child(&self.div(), snapshot);
                }
            }
        }
    }
}

glib::wrapper! {
    pub(crate) struct SplitView(ObjectSubclass<imp::SplitView>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl SplitView {
    /// Build a `SplitView` around `editor`, wrapping it in the ONE
    /// `GtkScrolledWindow` it will ever live in (the load-bearing invariant — see
    /// the module doc). Starts in `Preview` mode with the editor hidden; the
    /// caller installs the initial preview via [`set_preview`](Self::set_preview)
    /// and mounts the SplitView into the tab's `content_box`.
    pub(crate) fn new(editor: &sourceview::View) -> Self {
        let obj: Self = glib::Object::new();
        let imp = obj.imp();
        imp.fraction.set(0.5);
        // Fill `content_box` (a vertical GtkBox): without this the SplitView is
        // packed at its NATURAL height, which for its ScrolledWindow panes is only
        // a couple of lines tall, so the editor/preview would validate & paint just
        // the top ~2 lines. The old per-mode content children each set vexpand(true)
        // for the same reason; the persistent SplitView must carry it instead.
        obj.set_hexpand(true);
        obj.set_vexpand(true);

        // The editor's one-and-only scroller. `set_child` here fires the gutter's
        // vadjustment binding exactly ONCE (harmlessly — nothing has been freed
        // yet); it is never called again for this view.
        let editor_scroller = gtk::ScrolledWindow::builder()
            .child(editor)
            .hexpand(true)
            .vexpand(true)
            .build();
        // Wrap the scroller in a GtkOverlay so the editor pane can float an in-surface
        // Annotate card over it (GTK4Rs/AP-83), mirroring the preview pane. The OVERLAY is the
        // SplitView's direct child (set_parent once); the scroller is the overlay's main
        // child, set exactly once and never reparented — so the gutter's vadjustment
        // binding never re-fires (the reparent invariant is preserved: only *added*
        // overlay children ever change, never the scroller's parent).
        let editor_overlay = gtk::Overlay::new();
        editor_overlay.set_child(Some(&editor_scroller));
        editor_overlay.set_hexpand(true);
        editor_overlay.set_vexpand(true);
        editor_overlay.set_parent(&obj);
        // Wire the editor's Annotate card onto the overlay (persistent — wired once,
        // survives every mode switch since the editor is never rebuilt) and store the
        // trigger the `win.annotate` action calls when the editor pane is active.
        let editor_trigger =
            super::editor_annotate::wire_editor_annotate_card(&editor_overlay, editor);
        imp.editor_annotate_trigger.replace(Some(editor_trigger));
        let _ = imp.editor_scroller.set(editor_scroller);
        let _ = imp.editor_overlay.set(editor_overlay);

        let preview_holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
        preview_holder.set_hexpand(true);
        preview_holder.set_vexpand(true);
        preview_holder.set_parent(&obj);
        let _ = imp.preview_holder.set(preview_holder);

        let divider = gtk::Separator::new(gtk::Orientation::Vertical);
        divider.add_css_class("split-divider");
        divider.set_parent(&obj);
        let _ = imp.divider.set(divider);

        // Divider drag: a `GtkGestureDrag` on the SplitView (NOT on the divider).
        // Attaching to the non-moving SplitView keeps the gesture's along-axis
        // offset drift-free — a gesture on the divider would measure its offset in
        // the divider's own coordinate space, which shifts by exactly the amount
        // we move it each frame, producing the classic "sticky handle" self-cancel.
        // The begin handler denies any drag that doesn't start on the divider, so
        // ordinary editor/preview interaction (text selection, scrollbar drags) is
        // untouched — those children claim the sequence first anyway.
        let drag = gtk::GestureDrag::new();
        drag.connect_drag_begin(glib::clone!(
            #[weak(rename_to = obj)]
            obj,
            move |g, x, y| {
                if !obj.begin_divider_drag(x, y) {
                    g.set_state(gtk::EventSequenceState::Denied);
                }
            }
        ));
        drag.connect_drag_update(glib::clone!(
            #[weak(rename_to = obj)]
            obj,
            move |_g, off_x, off_y| {
                obj.update_divider_drag(off_x, off_y);
            }
        ));
        drag.connect_drag_end(glib::clone!(
            #[weak(rename_to = obj)]
            obj,
            move |_g, _, _| {
                obj.imp().dragging.set(false);
            }
        ));
        obj.add_controller(drag);

        // Seed the initial (Preview) visibility/cursor state.
        obj.apply_layout_state();
        obj
    }

    /// The editor's persistent `GtkScrolledWindow` (its child is the reused
    /// `GtkSourceView`). Stable for the tab's whole life — the split-sync and
    /// scroll-restore code wires to it once and never rewires (unlike the
    /// preview, which is rebuilt).
    pub(crate) fn editor_scroller(&self) -> gtk::ScrolledWindow {
        self.imp().ed()
    }

    /// Raise the editor pane's Annotate card over the current editor selection
    /// Returns `false` (a no-op) if the trigger isn't wired.
    /// The `win.annotate` action calls this when the editor is the active pane; the card
    /// then writes the created annotation into the editor source buffer.
    pub(crate) fn trigger_editor_annotate(&self) -> bool {
        let trigger = self.imp().editor_annotate_trigger.borrow().clone();
        if let Some(trigger) = trigger {
            trigger();
            true
        } else {
            false
        }
    }

    /// The current preview `GtkScrolledWindow`, or `None` when the preview has
    /// been freed (edit mode has no preview to render/zoom/spy on). The mounted
    /// preview PANE is a `GtkOverlay` (built by `preview::render`) wrapping the
    /// scroller so the CriticMarkup Annotate bar can float in-surface (GTK4Rs/AP-83); dig
    /// one level through it to the scroller every consumer expects.
    pub(crate) fn preview_scroller(&self) -> Option<gtk::ScrolledWindow> {
        self.imp()
            .holder()
            .first_child()
            .and_then(|c| c.downcast::<gtk::Overlay>().ok())
            .and_then(|o| o.child())
            .and_then(|c| c.downcast::<gtk::ScrolledWindow>().ok())
    }

    /// Install (or clear) the preview pane. Pass the freshly-`render`ed preview
    /// `GtkScrolledWindow` when entering a preview-visible mode; pass `None` when
    /// entering edit mode to free it. Only the preview is ever swapped here — the
    /// editor is never touched.
    pub(crate) fn set_preview(&self, preview: Option<&gtk::Widget>) {
        let holder = self.imp().holder();
        while let Some(old) = holder.first_child() {
            holder.remove(&old);
        }
        if let Some(p) = preview {
            holder.append(p);
            self.install_annotation_sink(p);
            // A freshly-mounted preview is empty-or-mount-content-validated; its FIRST
            // live re-render (`set_buffer` while already visible) is the one that can
            // blank the overlay. Arm the one-shot heal for that render.
            self.imp().preview_first_render_pending.set(true);
        }
    }

    /// Returns whether the next split-mode live re-render is the FIRST since a fresh
    /// preview mount (clearing the flag). The caller then arms the one-shot
    /// first-content repaint that heals a possible overlay blank.
    pub(crate) fn take_preview_first_render(&self) -> bool {
        self.imp().preview_first_render_pending.replace(false)
    }

    /// Wire the newly-mounted preview's CriticMarkup marker/overlay actions to
    /// mutate THIS tab's editor source buffer. The preview view
    /// is rebuilt on every mode switch / reload / tab-materialize, so the sink is
    /// (re)installed here — the one choke point every preview mount passes through
    /// — using the tab's stable editor buffer.
    fn install_annotation_sink(&self, preview: &gtk::Widget) {
        let Some(view) = preview
            .downcast_ref::<gtk::Overlay>()
            .and_then(|o| o.child())
            .and_then(|c| c.downcast::<gtk::ScrolledWindow>().ok())
            .and_then(|sw| sw.child())
            .and_then(|c| c.downcast::<crate::codeview::CodePreviewView>().ok())
        else {
            return;
        };
        let Some(editor) = self.editor_scroller().child() else {
            return;
        };
        let src_buf = editor
            .downcast::<sourceview::View>()
            .ok()
            .map(|v| v.buffer());
        let Some(src_buf) = src_buf else {
            return;
        };
        let buf_weak = src_buf.downgrade();
        let view_weak = view.downgrade();
        view.set_annotation_sink(std::rc::Rc::new(move |edit| {
            let Some(buf) = buf_weak.upgrade() else {
                return;
            };
            crate::window::apply_annotation_edit(&buf, edit);
            // The mutation edited the editor buffer; re-render the preview now so
            // the new highlight/marker/comment shows immediately (not just after a
            // reload) — the split-only live-preview debounce does not cover the
            // preview-only mode where annotations are created (§17.5–17.8 fix).
            if let Some(v) = view_weak.upgrade() {
                crate::window::refresh_preview_after_annotation(&v);
            }
        }));
    }

    /// Apply a full layout: mode, split orientation, and split order. Rebuilds no
    /// widgets and reparents nothing — pure visibility + relayout.
    pub(crate) fn set_layout(&self, mode: ViewMode, vertical: bool, swapped: bool) {
        let imp = self.imp();
        imp.mode.set(mode);
        imp.vertical.set(vertical);
        imp.swapped.set(swapped);
        self.apply_layout_state();
    }

    /// Update just the split order (win.split-swap). Reparent-free — the old
    /// design re-entered split mode to rebuild the `GtkPaned`, which for a
    /// *vertical* split reparented the editor and re-triggered the reparent UAF.
    pub(crate) fn set_swapped(&self, swapped: bool) {
        self.imp().swapped.set(swapped);
        self.apply_layout_state();
    }

    /// Update just the split orientation (win.split-orientation), in place.
    pub(crate) fn set_vertical(&self, vertical: bool) {
        self.imp().vertical.set(vertical);
        self.apply_layout_state();
    }

    /// Reflect the current (mode, vertical, swapped) into child visibility, the
    /// divider orientation/cursor, and a resize request. Called after any layout
    /// change and once at construction.
    fn apply_layout_state(&self) {
        let imp = self.imp();
        let mode = imp.mode.get();
        let vertical = imp.vertical.get();
        let split = mode == ViewMode::Split;

        // A hidden pane is `set_child_visible(false)` — unmapped, NEVER
        // reparented, so the editor's gutter vadjustment binding never re-fires.
        // It also drops out of `find_text_view`'s active-view walk
        // (window/actions.rs skips child-invisible subtrees), so preview-mode Copy
        // binds to the preview, not the hidden editor. Hide the editor OVERLAY (the
        // SplitView's direct child that wraps the scroller), so the whole editor
        // subtree — scroller + Annotate card — is unmapped and skipped in one step.
        imp.edpane().set_child_visible(mode.is_editor_visible());
        imp.holder().set_child_visible(mode.is_preview_visible());
        let divider = imp.div();
        divider.set_child_visible(split);
        if split {
            // Horizontal split (side-by-side) → a vertical separator line, and a
            // column-resize cursor; vertical split → a horizontal line + row cursor.
            divider.set_orientation(if vertical {
                gtk::Orientation::Horizontal
            } else {
                gtk::Orientation::Vertical
            });
            divider.set_cursor_from_name(Some(if vertical { "row-resize" } else { "col-resize" }));
        }
        // Min/nat sizes differ between single-pane and split, so request a full
        // resize, not just a re-allocate.
        self.queue_resize();
    }

    /// Begin a divider drag if `(x, y)` (SplitView-local) is on the divider in
    /// split mode. Records the first pane's current length as the drag anchor.
    fn begin_divider_drag(&self, x: f64, y: f64) -> bool {
        let imp = self.imp();
        if imp.mode.get() != ViewMode::Split {
            return false;
        }
        let alloc = imp.div().allocation();
        // Hit-test the divider rect widened by HANDLE_SLOP on each side.
        let (dx, dy, dw, dh) = (
            alloc.x() as f64 - HANDLE_SLOP,
            alloc.y() as f64 - HANDLE_SLOP,
            alloc.width() as f64 + 2.0 * HANDLE_SLOP,
            alloc.height() as f64 + 2.0 * HANDLE_SLOP,
        );
        if x < dx || x > dx + dw || y < dy || y > dy + dh {
            return false;
        }
        // Anchor = the first pane's current on-screen length (== the divider's
        // start offset along the split axis).
        let anchor = if imp.vertical.get() {
            alloc.y() as f64
        } else {
            alloc.x() as f64
        };
        imp.drag_anchor.set(anchor);
        imp.dragging.set(true);
        true
    }

    /// Update the split fraction from an in-progress divider drag (offset is from
    /// the drag-begin point, in SplitView-local space — drift-free).
    fn update_divider_drag(&self, off_x: f64, off_y: f64) {
        let imp = self.imp();
        if !imp.dragging.get() {
            return;
        }
        let vertical = imp.vertical.get();
        let axis_len = if vertical {
            self.height()
        } else {
            self.width()
        } as f64;
        let avail = (axis_len - HANDLE_SIZE).max(0.0);
        if avail <= 0.0 {
            return;
        }
        let delta = if vertical { off_y } else { off_x };
        let first_len = (imp.drag_anchor.get() + delta).clamp(0.0, avail);
        imp.fraction.set(first_len / avail);
        // Size hasn't changed — only where the divider sits — so re-allocate
        // rather than re-measure (the clamp against pane minima happens in
        // `size_allocate`, so an over-drag simply pins at the limit).
        self.queue_allocate();
    }
}
