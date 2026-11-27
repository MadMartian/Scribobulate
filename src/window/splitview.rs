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
        /// when the editor is the active pane. Captures only weak refs (no cycle, GTK4Rs/AP-63).
        pub(super) editor_annotate_trigger: RefCell<Option<crate::codeview::AnnotateTrigger>>,
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
        relink_vscrollbar_after_range_changes(&editor_scroller);
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
    /// The overlay the editor pane floats its cards over (the formatting bar, the
    /// Annotate card).
    pub(crate) fn editor_overlay(&self) -> gtk::Overlay {
        self.imp().edpane()
    }

    pub(crate) fn editor_scroller(&self) -> gtk::ScrolledWindow {
        self.imp().ed()
    }

    /// Raise the editor pane's Annotate card over the current editor selection
    /// Returns `false` (a no-op) if the trigger isn't wired.
    /// The `win.annotate` action calls this when the editor is the active pane; the card
    /// then writes the created annotation into the editor source buffer.
    pub(crate) fn trigger_editor_annotate(&self, kind: crate::preview::AnnotateKind) -> bool {
        let trigger = self.imp().editor_annotate_trigger.borrow().clone();
        if let Some(trigger) = trigger {
            trigger(kind);
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

    /// Apply the app-wide split arrangement (`window::arrangement`) in place.
    /// Reparent-free in both halves: the old design re-entered split mode to
    /// rebuild a `GtkPaned`, which for a *vertical* split reparented the editor and
    /// re-triggered the reparent UAF (ScrAP-58); and the scroll-sync projection is
    /// fraction-based, so an axis change needs no rewire (GTK4Rs/AP-16). Harmless
    /// outside split mode, where neither half is read.
    pub(crate) fn set_arrangement(&self, arrangement: super::arrangement::SplitArrangement) {
        let imp = self.imp();
        imp.swapped.set(arrangement.swapped);
        imp.vertical.set(arrangement.vertical);
        self.apply_layout_state();
    }

    /// Read back the arrangement this widget is showing. Test-only: production code
    /// only ever WRITES it, and a test needs to see the effect on the widget itself,
    /// not the value it was told to apply. Carries the same cfg as its only callers
    /// (POLICY's helper-gating rule), not a bare `#[cfg(test)]`.
    #[cfg(all(test, feature = "gtk-integration-tests"))]
    pub(crate) fn arrangement(&self) -> super::arrangement::SplitArrangement {
        let imp = self.imp();
        super::arrangement::SplitArrangement {
            swapped: imp.swapped.get(),
            vertical: imp.vertical.get(),
        }
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
        // Only where the divider sits changed, but ask for a resize anyway: a bare
        // `queue_allocate` can be dropped on GTK 4.6 and leave the panes undrawn
        // (GTK4Rs/AP-104). The clamp against pane minima happens in `size_allocate`,
        // so an over-drag simply pins at the limit.
        self.queue_resize();
    }
}

/// Re-attach the editor's vertical scrollbar to GTK's layout after its range or position
/// changes — the repair for a scrollbar that stops being drawn after an edit adds a line.
///
/// On GTK 4.6 every change to the scroll range or position asks for the scrollbar's trough
/// to be laid out again. When that request lands while an ancestor is still mid-layout, the
/// ancestor clears its "a child needs layout" flag on the way out and the trough's request
/// is orphaned: GTK skips painting it (`Trying to snapshot GtkGizmo … without a current
/// allocation`) and no same-size relayout reaches it, only a real size change. Fixed
/// upstream in GTK 4.10.0 (merge request !5564); GTK 4.22 still shows a milder form that
/// heals on the next scroll (GNOME/gtk#6057). Runs on every version.
///
/// MEASURED (GTK 4.6.9, scripted Enter-then-scroll drive): 24 of 25 edits lost the
/// scrollbar without this, 0 of 30 with it. Only a hide/show repairs it. A same-size
/// `size_allocate` and a one-pixel size bump both measured 24 of 25 — the orphaned trough
/// is below a widget whose own flags are already clear, so neither reaches it. Hooked to
/// `value-changed` as well as `changed`: on `changed` alone it measured 11 of 12, because
/// the orphaning also happens when the view scrolls. Repeated once ~120 ms later because
/// one pass measured 1 of 30. Cost measured as nil: CPU over 300 wheel notches on a
/// 4,000-line document varied more between runs than between on and off, and the
/// scrollbar stays painted through a continuous scroll.
///
/// Known cost, accepted by the operator as negligible: the hide/show resets GTK's record
/// that the pointer is over the scrollbar, so a pointer resting perfectly still on it no
/// longer holds the overlay scrollbar shown after an edit — it fades 1–2 s later, and any
/// motion brings it back (measured on Windows, GTK 4.22.4). On GTK 4.22 the repair also
/// only reduces a milder, separate lag (gtk#6057): measured 7 of 40 Enters to 2 of 40.
///
/// Coalesced to one repair per main-loop turn, and run from a timeout, never inside a
/// layout pass, where toggling `:visible` would itself be the hazard (GTK4Rs/AP-104).
///
/// **Never while a button is held on the scrollbar.** Hiding the bar unmaps its
/// `GtkRange`, and `gtk_range_unmap` calls `stop_scrolling` (4.6.9 `gtkrange.c:1640`),
/// which drops the slider grab and the trough's step timer. A thumb drag would then stop
/// following the pointer after its first motion, because that motion's `value-changed`
/// is what queues the repair. While pressed the repair is held, and it runs once after
/// the release. The press is read by a capture-phase legacy controller, which sees the
/// release even after the range's own drag gesture has claimed the sequence (a click
/// gesture that is denied gets no `released`, GTK4Rs/AP-169).
fn relink_vscrollbar_after_range_changes(scroller: &gtk::ScrolledWindow) -> RelinkGate {
    let gate = RelinkGate::install(&scroller.vscrollbar());
    let wired = gate.clone();
    let pending = std::rc::Rc::new(std::cell::Cell::new(false));
    let weak = scroller.downgrade();
    let schedule = move |_: &gtk::Adjustment| {
        if pending.replace(true) {
            return;
        }
        let pending = pending.clone();
        let weak = weak.clone();
        let gate = gate.clone();
        glib::idle_add_local_once(move || {
            pending.set(false);
            let Some(bar) = weak.upgrade().map(|s| s.vscrollbar()) else {
                return;
            };
            gate.relink_twice(&bar);
        });
    };
    let adj = scroller.vadjustment();
    let on_value = schedule.clone();
    adj.connect_value_changed(move |a| on_value(a));
    adj.connect_changed(move |a| schedule(a));
    wired
}

/// Holds the scrollbar repair while a pointer button is down on the bar, and runs the
/// held repair after the release (or a broken grab, or the bar unmapping). See [`relink_vscrollbar_after_range_changes`].
#[derive(Clone, Default)]
struct RelinkGate {
    pressed: std::rc::Rc<std::cell::Cell<bool>>,
    held: std::rc::Rc<std::cell::Cell<bool>>,
}

impl RelinkGate {
    fn install(bar: &gtk::Widget) -> Self {
        let gate = Self::default();
        let legacy = gtk::EventControllerLegacy::new();
        legacy.set_propagation_phase(gtk::PropagationPhase::Capture);
        let on_event = gate.clone();
        let weak_bar = bar.downgrade();
        legacy.connect_event(move |_, event| {
            match event.event_type() {
                gdk::EventType::ButtonPress | gdk::EventType::TouchBegin => {
                    on_event.pressed.set(true);
                }
                // A broken grab is a release that never arrives as one: the press
                // would otherwise hold the repair off for good.
                gdk::EventType::ButtonRelease
                | gdk::EventType::TouchEnd
                | gdk::EventType::TouchCancel
                | gdk::EventType::GrabBroken => on_event.release(&weak_bar),
                _ => {}
            }
            glib::Propagation::Proceed
        });
        bar.add_controller(legacy);
        // An unmapped bar receives no release either (the pane hidden, the mode switched
        // mid-press), so unmapping ends the press too.
        let on_unmap = gate.clone();
        bar.connect_unmap(move |bar| on_unmap.release(&bar.downgrade()));
        gate
    }

    /// The release half: clear the press and, if a repair was held, run it from an idle
    /// rather than inside the bar's own event emission (GTK4Rs/AP-30).
    fn release(&self, bar: &glib::WeakRef<gtk::Widget>) {
        self.pressed.set(false);
        if !self.held.replace(false) {
            return;
        }
        let bar = bar.clone();
        let gate = self.clone();
        glib::idle_add_local_once(move || {
            if let Some(bar) = bar.upgrade() {
                gate.relink_twice(&bar);
            }
        });
    }

    /// The repair as measured to be needed: once now and once [`RELINK_REPEAT`] later,
    /// because one pass alone measured 1 of 30 left orphaned. A held repair replays both
    /// passes, not one.
    fn relink_twice(&self, bar: &gtk::Widget) {
        self.relink(bar);
        let bar = bar.downgrade();
        let gate = self.clone();
        glib::timeout_add_local_once(RELINK_REPEAT, move || {
            if let Some(bar) = bar.upgrade() {
                gate.relink(&bar);
            }
        });
    }

    /// Repair now, or hold the repair until the release when a button is down.
    fn relink(&self, bar: &gtk::Widget) {
        if self.pressed.get() {
            self.held.set(true);
            return;
        }
        relink_vscrollbar(bar);
    }
}

/// How long after the first repair the second runs.
const RELINK_REPEAT: std::time::Duration = std::time::Duration::from_millis(120);

/// Hide then show the scrollbar, which re-lays it out from a zero size. Does nothing to a
/// scrollbar that is not on screen.
fn relink_vscrollbar(bar: &gtk::Widget) {
    if !bar.is_mapped() {
        return;
    }
    bar.set_visible(false);
    bar.set_visible(true);
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::relink_vscrollbar_after_range_changes;
    use crate::testpump::{drain_for, until, Clock};
    use gtk::prelude::*;
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Duration;

    /// A mapped scrolled window around a text view long enough to scroll, with the repair
    /// wired, and a counter of the vertical scrollbar's hide/show transitions.
    fn scroller_with_counter() -> (gtk::Window, gtk::ScrolledWindow, Rc<Cell<u32>>) {
        let (window, scroller, toggles, _) = scroller_with_gate();
        (window, scroller, toggles)
    }

    /// [`scroller_with_counter`], also returning the press gate the repair was wired with.
    fn scroller_with_gate() -> (
        gtk::Window,
        gtk::ScrolledWindow,
        Rc<Cell<u32>>,
        super::RelinkGate,
    ) {
        let view = gtk::TextView::new();
        let body: String = (0..400).map(|i| format!("line {i}\n")).collect();
        view.buffer().set_text(&body);
        let scroller = gtk::ScrolledWindow::builder().child(&view).build();
        let gate = relink_vscrollbar_after_range_changes(&scroller);
        let window = gtk::Window::new();
        window.set_default_size(400, 300);
        window.set_child(Some(&scroller));
        window.present();
        until(Clock::Frame, "the scrollbar maps", || {
            scroller.vscrollbar().is_mapped() && scroller.vadjustment().page_size() > 0.0
        });
        drain_for(Clock::Frame, Duration::from_millis(400));
        let toggles = Rc::new(Cell::new(0));
        let t = toggles.clone();
        scroller
            .vscrollbar()
            .connect_notify_local(Some("visible"), move |_, _| t.set(t.get() + 1));
        (window, scroller, toggles, gate)
    }

    /// TDD 4.14 — the editor's scrollbar is re-laid out after its range changes AND after
    /// its position changes, twice each time. Both triggers are load-bearing: the orphaned
    /// trough this repairs was measured forming on a scroll as well as on an edit, and the
    /// repair hooked to the range alone left 11 of 12 edits without a scrollbar.
    ///
    /// The orphaning itself cannot be reproduced in a test process (GTK 4.6's layout
    /// ordering is not driven here), so this pins the repair's wiring; the effect is the
    /// manual check MANUAL-TEST 4.14. Mutation: dropping either `connect_*` fails the
    /// matching half; dropping the repeat halves the count.
    #[gtktest::test]
    fn a_range_or_position_change_relinks_the_scrollbar_twice() {
        let (window, scroller, toggles) = scroller_with_counter();
        let adj = scroller.vadjustment();

        let before = toggles.get();
        crate::saferizer::scrollpos::jump(&adj, adj.value() + 40.0);
        drain_for(Clock::Frame, Duration::from_millis(400));
        assert_eq!(
            toggles.get() - before,
            4,
            "a scroll must hide and show the scrollbar twice (two transitions each)"
        );

        // `changed` emitted on its own, with no value change: moving `upper` instead let
        // the view's own reconfiguration move the value too, so the value hook alone
        // could satisfy this half.
        let before = toggles.get();
        adj.emit_by_name::<()>("changed", &[]);
        drain_for(Clock::Frame, Duration::from_millis(400));
        assert_eq!(
            toggles.get() - before,
            4,
            "a range change must relink the scrollbar too"
        );
        assert!(
            scroller.vscrollbar().is_visible(),
            "the scrollbar ends visible"
        );
        window.destroy();
    }

    /// While a button is held on the scrollbar, the repair must not hide it: unmapping
    /// the `GtkRange` drops the slider grab (`gtk_range_unmap` → `stop_scrolling`), and a
    /// thumb drag stops after its first motion. The held repair runs once on release.
    ///
    /// The press is set on the gate directly: synthesising a pointer press into a
    /// headless test is not possible here, so the classification of real events is the
    /// manual check MANUAL-TEST 4.14. Mutation: dropping the `pressed` check in
    /// `RelinkGate::relink` fails the first assertion; dropping the held replay in
    /// `release` fails the second.
    #[gtktest::test]
    fn a_held_scrollbar_is_not_relinked_until_the_release() {
        let (window, scroller, toggles, gate) = scroller_with_gate();
        let adj = scroller.vadjustment();

        gate.pressed.set(true);
        let before = toggles.get();
        crate::saferizer::scrollpos::jump(&adj, adj.value() + 40.0);
        drain_for(Clock::Frame, Duration::from_millis(400));
        assert_eq!(toggles.get(), before, "a held scrollbar must not be hidden");

        gate.release(&scroller.vscrollbar().downgrade());
        drain_for(Clock::Frame, Duration::from_millis(400));
        assert_eq!(
            toggles.get() - before,
            4,
            "the held repair must run after the release, both passes of it"
        );
        assert!(
            scroller.vscrollbar().is_visible(),
            "the scrollbar ends visible"
        );
        window.destroy();
    }

    /// A press whose release never reaches the bar (it was unmapped mid-press) must
    /// not hold the repair off for good: unmapping ends the press.
    ///
    /// Mutation: drop the `connect_unmap` in `RelinkGate::install` and the press stays.
    #[gtktest::test]
    fn unmapping_the_bar_ends_a_press_whose_release_never_came() {
        let (window, scroller, _toggles, gate) = scroller_with_gate();
        gate.pressed.set(true);
        let bar = scroller.vscrollbar();
        bar.set_visible(false);
        bar.set_visible(true);
        assert!(
            !gate.pressed.get(),
            "the press must end when the bar unmaps"
        );
        window.destroy();
    }
}
