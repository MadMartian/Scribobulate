//! `CodePreviewView` — a `GtkTextView` subclass for the Markdown preview that
//! draws each fenced code block's background itself, *under* the text.
//!
//! A `paragraph-background` tag cannot give a code block inner horizontal
//! padding: GTK pins the fill's left edge to the text's `left_margin` (they are
//! literally the same x), so the text is always flush with the colored edge
//! (GTK4Rs/AP-21).  The fix is to draw each block rectangle in `TextViewImpl::snapshot_layer`
//! at the BelowText layer (above the widget background, below the text), and to
//! inset the text past the rect via the `code-block` tag's margins.
//!
//! The rectangles' vertical extents come from `line_yrange` (a pure cached
//! btree-height read), NOT `iter_location`. Two reasons, both load-bearing:
//! (1) GTK4Rs/AP-22 — a geometry read that *validates* a line's layout on demand, forced on
//! an OFF-SCREEN line during a draw/size-allocate, sets `alloc_needed` mid-cycle and
//! blanks the view. So we measure **live in `snapshot_layer`, visible blocks only**,
//! clamping each rectangle to the viewport. (2) ScrAP-105 — `iter_location` also builds
//! and *caches* a line DISPLAY, inserting it into the view's line-display GSequence
//! (kept sorted by line number); right after a `set_buffer` swap that cache still
//! holds displays whose `GtkTextLine`s were freed with the old buffer, and the
//! insert's comparator dereferences a freed line → SIGSEGV. A paint can land in that
//! window (before the post-swap re-validation), so even a visible-only `iter_location`
//! is unsafe here. `line_yrange` touches no display cache and validates nothing, and
//! on a validated on-screen line yields the same top/bottom y — so it is immune to
//! both hazards. Coordinates are buffer-space — the same space `snapshot_layer` draws
//! in — so it is scroll-correct for free.
//!
//! Corners are square (angled), per the desired look.

use crate::saferizer::viewport::ViewportRange;
use gtk::prelude::*;
use gtk::{gdk, glib, graphene};

/// `GtkTextView`'s `SPACE_FOR_CURSOR` — the 1 logical pixel it adds **once** to its
/// global layout width (`width += SPACE_FOR_CURSOR`, gtktextview.c) so the cursor at a
/// line end is never clipped. Wrapped text already wraps to `content - 1`, so it never
/// overflows; an anchored child's advance can't be trimmed, so a child sized to
/// exactly the content column makes the layout 1px past the viewport and raises a
/// spurious horizontal scrollbar that churns the GTK4Rs/AP-23 blank. Bounding every anchored
/// block child to `content - this` makes it behave like a wrapped line. The value is a
/// fixed literal in GTK (invariant to font / scale / HiDPI / RTL / multiple anchors —
/// researcher-verified against gtk-4-6), so a named constant is exact; do NOT derive
/// it from cursor geometry (the strong cursor's own width is 0).
const ANCHORED_LINE_END_SLACK: i32 = 1;

mod card;
mod geometry;
mod gutter;
mod markers;
mod navkeys;

pub(crate) use geometry::move_or_create_mark;
pub(crate) use markers::{
    AnnotateTrigger, AnnotationEdit, AnnotationSink, CreateAnnotation, MarkerData, MarkerSource,
    PendingMarkerOpen,
};

mod imp {
    use super::geometry::{chip_rect, marker_row_y_h, span_card_y_extent};
    use super::gutter::{
        draw_list_marker, first_display_line, list_content_margin_px, marker_gap_px, MarkerPaint,
    };
    use super::markers::group_by_line;
    use super::*;
    use gtk::subclass::prelude::*;
    use std::cell::{Cell, RefCell};

    pub(crate) struct CodePreviewView {
        /// (first-char offset, exclusive end offset) per fenced code block.
        pub(crate) blocks: RefCell<Vec<crate::span::BufferSpan>>,
        pub(crate) bg: RefCell<gdk::RGBA>,
        /// (first-char offset, exclusive end offset) per blockquote, + the accent-bar
        /// colour. Blockquotes are buffer text now; the view draws the left bar over
        /// each range in snapshot_layer (no anchored widget to churn — GTK4Rs/AP-23).
        pub(crate) blockquotes: RefCell<Vec<crate::span::BufferSpan>>,
        pub(crate) bq_bar: RefCell<gdk::RGBA>,
        /// Replaceable idle for a pending scroll-to-heading, so rapid outline
        /// re-targeting collapses to a single scroll of the latest target.
        pub(crate) scroll_idle: RefCell<Option<glib::SourceId>>,
        /// Anchored children that must be bounded to the live content-column width
        /// (tables, rules, blockquotes), each with the fixed chrome to its left
        /// (`inset`). Bound on every `size_allocate` — see [`super::CodePreviewView::set_width_bounded`].
        pub(crate) width_bounded: RefCell<Vec<(gtk::Widget, i32)>>,
        /// Custom `ScribTableWidget`s anchored in this view. On each real
        /// content-column change `size_allocate` calls their `set_bound_width` (each
        /// is itself idempotent), driving their once-per-width cached layout (GTK4Rs/AP-23).
        pub(crate) tables: RefCell<Vec<crate::widgets::table::ScribTableWidget>>,
        /// Anchored images, each `(picture, max_w, max_h)` — the image's NATURAL size
        /// (uncapped: `max-width: 100%` policy). On each real content-column change
        /// `size_allocate` CLAMPS each to `w = min(max_w, content)`, `h = max_h · w /
        /// max_w` (aspect preserved), so a too-wide image shrinks to the viewport instead
        /// of forcing an over-wide line and re-arming the GTK4Rs/AP-22/23 blank, and
        /// an image narrower than the pane keeps its natural size (never upscaled).
        /// Distinct from `width_bounded`, which FILLS the column. Both width AND height
        /// stay non-zero so the picture paints (GTK4Rs/AP-58).
        pub(crate) image_bounded: RefCell<Vec<(gtk::Widget, i32, i32)>>,
        /// Last content-column width applied; the idempotent guard that keeps the
        /// `size_allocate` rebind from looping.
        pub(crate) last_content_width: Cell<i32>,
        /// Last raw ALLOCATION width seen in `size_allocate`. Distinct from
        /// `last_content_width` (= width − margins): a horizontal resize changes the
        /// allocation width and re-wraps the text (Reading-Position Preservation CAM,
        /// row 7); zoom changes only the margins at the SAME width and owns its own
        /// scroll restore, so keying the reading-line re-anchor on the raw width (not
        /// content) isolates a true geometry resize from a zoom margin change. `-1`
        /// until the first allocation, so the first real width is not treated as a
        /// resize (nothing to preserve; must not fight the initial fresh render).
        pub(crate) last_alloc_width: Cell<i32>,
        /// The buffer line a programmatic scroll (zoom-restore or outline nav) is
        /// heading to, cached so a rapid zoom burst re-anchors to the intended
        /// reading position instead of re-reading the transient adjustment value
        /// mid-restore — the restore idle + `scroll_to_mark` animation cannot
        /// settle between fast successive zooms, so the live top line briefly
        /// reads ≈ 0 and the preview drifts to the top (ANTI-PATTERNS
        /// ScrAP-65). Tracks the user's settled top line while `user_scrolling`, and a
        /// programmatic scroll's target otherwise; `None` only before any scroll,
        /// where the live top line (line 0) is trustworthy.
        pub(crate) restore_target_line: Cell<Option<i32>>,
        /// True once the user has scrolled this view by hand (wheel/touchpad) and
        /// until the next programmatic scroll takes over. While set, `value-changed`
        /// records the settled top line into `restore_target_line`, so a following
        /// zoom re-anchors to where the user actually left off rather than a live,
        /// mid-scroll-animation value (ScrAP-65).
        pub(crate) user_scrolling: Cell<bool>,
        /// CriticMarkup comment markers drawn in the right margin.
        pub(crate) markers: RefCell<Vec<MarkerData>>,
        /// List-item markers drawn in the LEFT gutter — a
        /// bullet dot / right-aligned number / static checkbox per item, aligned to
        /// the item's first line. Not buffer text (so selection/copy skip them);
        /// painted live in `snapshot_layer(BelowText)` with cache-free `line_yrange`.
        pub(crate) list_markers: RefCell<Vec<crate::renderer::ListMarker>>,
        /// The zoom factor the current `list_markers` were laid out at — stored
        /// alongside them so the drawn marker x (`px(depth*28)`) tracks the `li-{depth}`
        /// content margin at any zoom (the content margin scales; the marker must too).
        pub(crate) gutter_zoom: Cell<f64>,
        /// Widget-coordinate hit-boxes recorded on each paint: `(rect, annotation
        /// indices on that line)`, so a click can map to the marker's annotations
        /// without re-measuring layout off the paint path.
        pub(crate) marker_hitboxes: RefCell<Vec<(graphene::Rect, Vec<usize>)>>,
        /// An armed "open marker `target`'s popover the moment its chip paints" request.
        /// A programmatic navigation to an OFF-SCREEN marker cannot open a
        /// popover immediately: `marker_hitboxes` is repopulated only by `snapshot_layer`
        /// and therefore holds only the VISIBLE markers (GTK4Rs/AP-97), so the target has no
        /// rect to point at until it has been scrolled to AND painted.
        ///
        /// GTK offers no signal for "that paint happened": `GtkTextView` has no
        /// validate/paint signal at all (its signals are keybinding/action only), the
        /// `GtkTextLayout` that *does* emit `changed`/`invalidated` is not public GTK4
        /// API, and `GDK_FRAME_CLOCK_PHASE_AFTER_PAINT` is documented "should not be
        /// handled by applications". So the completion event is **self-generated**: we
        /// own the paint, and `snapshot_layer` fires this request the instant the target's
        /// hit-box exists — an event at exactly the goal state, not a poll racing a guess.
        pub(crate) pending_marker_open: RefCell<Option<PendingMarkerOpen>>,
        /// Monotonic navigation token. Every call to `open_marker_popover_at` bumps it
        /// and captures the new value in its converge-scroll tick; a tick whose captured
        /// token no longer equals this **has been superseded by a newer navigation** and
        /// stops at once, instead of continuing to re-aim the shared vadjustment at its
        /// now-stale target. Without it, rapid activations leave two (or more) converge
        /// ticks fighting over the one adjustment — the older one keeps pulling the scroll
        /// back to a previous target, so the document lands somewhere that matches neither
        /// the selection nor the last activation (a scroll/selection desync that only
        /// surfaces under fast interaction — the annotations viewer's click-through path).
        /// `pending_marker_open.is_none()` cannot catch this: a new navigation *replaces*
        /// the pending request rather than clearing it, so the old tick never sees `None`.
        pub(crate) nav_generation: Cell<u64>,
        /// Bumped once per render of this view's content — the identity of "what the
        /// preview currently shows".
        ///
        /// It exists because the obvious key for that, the `GtkTextBuffer`'s object
        /// identity, is no longer available: a re-render rebuilds the view's OWN buffer
        /// rather than swapping in a new one (handing a live `GtkTextView` a different
        /// buffer strands its layout's cached line displays on the freed one and the next
        /// cache insert faults — see `preview::build::build_render_products_into`). Any
        /// state derived from the rendered content — the find hit list is the standing
        /// case — keys on this instead, and because the bump lives in the same choke point
        /// that rebuilds the content, no render path can forget to invalidate.
        ///
        /// Strictly better than the identity it replaces, too: identity answered "is this
        /// a different buffer object", which was only ever a proxy for "is this a
        /// different render".
        pub(crate) render_generation: Cell<u64>,
        /// Widget-coordinate hit-boxes for the interactive TASK checkboxes only,
        /// each `(rect, index into list_markers)`. Recorded in
        /// the SAME convention as `marker_hitboxes` — buffer-x, y shifted by the scroll
        /// offset (`buffer_y - vtop`) — and cleared+repopulated for the VISIBLE task
        /// items on every `snapshot_layer(BelowText)` paint. `is_over_checkbox` tests a
        /// widget-space point against it (bullets/numbers are NOT recorded — only
        /// checkboxes are interactive). The rect is the shared pure `checkbox_rect`, so
        /// the hover band always matches the drawn box.
        pub(crate) checkbox_hitboxes: RefCell<Vec<(graphene::Rect, usize)>>,
        /// The list-marker index of the checkbox the pointer is currently over, or
        /// `None`. Drives the accent hover border in the draw loop; the motion handler
        /// only repaints (`queue_draw`) when this identity actually changes, so ordinary
        /// pointer movement doesn't thrash the paint.
        pub(crate) hovered_checkbox: Cell<Option<usize>>,
        /// The window-installed sink that performs an annotation mutation on the
        /// document source; `None` until the window wires it (then Edit/Remove show).
        pub(crate) annotation_sink: RefCell<Option<super::AnnotationSink>>,
        /// Reveals the comment-entry card over the current preview selection —
        /// stored by the create overlay so the `win.annotate` action triggers the
        /// same card as the popover's Annotate button.
        pub(crate) annotate_trigger: RefCell<Option<super::AnnotateTrigger>>,
        /// The selection action popover (Annotate, …), and the current marker popover
        /// (Edit/Remove on a CriticMarkup marker click). A `GtkPopover` attached via
        /// `set_parent` is a native child GTK does NOT unparent for us — we must do it
        /// in `dispose`, or the view's teardown spams "GtkPopover is not a child of
        /// ScribCodePreviewView" (GTK4Rs/AP-80). (The Annotate comment ENTRY is a GtkOverlay
        /// child, not a popover — GTK unparents overlay children itself — GTK4Rs/AP-83.)
        pub(crate) overlay_popover: RefCell<Option<crate::saferizer::PersistentPopover>>,
        /// The persistent annotation card. Not a bare `gtk::Popover` and not a
        /// `PersistentPopover`: [`AnnotationCard`](super::card::AnnotationCard) is a
        /// `GtkPopover` **subclass** that owns its own anchoring (identity, never a
        /// captured rect), its scroll-tracking, its outside-click dismissal and its
        /// teardown order — the three defects those used to be spread across the callers
        /// as three separate defects. Created once and reused for the view's life
        /// (GTK4Rs/AP-117/GTK4Rs/AP-117:
        /// destroying a popover per use strands a tooltip timer over an unrealized
        /// surface); torn down by `dispose` via `AnnotationCard::teardown`.
        pub(crate) marker_card: RefCell<Option<super::card::AnnotationCard>>,
        /// One-shot guard: has the persistent marker popover been PRE-WARMED (first
        /// `set_parent`+`popup`/`popdown` done while the view sat at scroll 0)? The
        /// first-EVER `set_parent`+`popup` of a popover parented to this view forces
        /// the view's first FULL size-allocate/validation of a tall anchored table,
        /// which changes the vadjustment and makes `validate_onscreen` re-anchor —
        /// a one-shot SCROLL JUMP that also shifts the just-clicked chip's hitbox out
        /// from under the grab (so the first click both scrolls AND sometimes fails to
        /// present). Absorbing it once at first map (scroll 0 → no visible jump) makes
        /// every real click hit a warm, already-validated popover (researcher-sourced,
        /// gtktextview.c:4655/4707/4746 validate_onscreen "odd side effect: scroll").
        pub(crate) marker_popover_warmed: Cell<bool>,
        /// The `changed` handler the create overlay connects on the DISPLAY-level
        /// primary clipboard to detect table-cell `GtkLabel` selections (table-cell annotation):
        /// a cell selection fires no buffer signal (selection island — ScrAP-110), so the
        /// only cross-environment signal is the primary clipboard's `changed`. The
        /// primary clipboard is long-lived (outlives the view), so we keep the id +
        /// clipboard here and disconnect in `dispose` — otherwise every fresh render's
        /// view would leak a handler on the shared clipboard (same discipline as the
        /// window's `copy-primary-handler`, GTK4Rs/AP-28/GTK4Rs/AP-82).
        pub(crate) primary_sel_handler: RefCell<Option<(gdk::Clipboard, glib::SignalHandlerId)>>,
    }

    impl Default for CodePreviewView {
        fn default() -> Self {
            // Transparent until set_code_blocks supplies the theme fill; `blocks`
            // is empty so nothing is drawn until then anyway.
            Self {
                blocks: RefCell::new(Vec::new()),
                bg: RefCell::new(gdk::RGBA::new(0.0, 0.0, 0.0, 0.0)),
                blockquotes: RefCell::new(Vec::new()),
                bq_bar: RefCell::new(gdk::RGBA::new(0.0, 0.0, 0.0, 0.0)),
                scroll_idle: RefCell::new(None),
                width_bounded: RefCell::new(Vec::new()),
                tables: RefCell::new(Vec::new()),
                image_bounded: RefCell::new(Vec::new()),
                last_content_width: Cell::new(-1),
                last_alloc_width: Cell::new(-1),
                restore_target_line: Cell::new(None),
                user_scrolling: Cell::new(false),
                markers: RefCell::new(Vec::new()),
                list_markers: RefCell::new(Vec::new()),
                gutter_zoom: Cell::new(1.0),
                marker_hitboxes: RefCell::new(Vec::new()),
                pending_marker_open: RefCell::new(None),
                nav_generation: Cell::new(0),
                render_generation: Cell::new(0),
                checkbox_hitboxes: RefCell::new(Vec::new()),
                hovered_checkbox: Cell::new(None),
                annotation_sink: RefCell::new(None),
                annotate_trigger: RefCell::new(None),
                overlay_popover: RefCell::new(None),
                marker_card: RefCell::new(None),
                marker_popover_warmed: Cell::new(false),
                primary_sel_handler: RefCell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CodePreviewView {
        const NAME: &'static str = "ScribCodePreviewView";
        type Type = super::CodePreviewView;
        type ParentType = gtk::TextView;
    }

    impl ObjectImpl for CodePreviewView {
        /// Unparent our `set_parent`-attached popovers before the view is
        /// destroyed. GTK does not auto-unparent native popover children, so
        /// without this teardown floods "GtkPopover is not a child of
        /// ScribCodePreviewView" (surfaced by the gtk-integration tests).
        fn dispose(&self) {
            if let Some(p) = self.overlay_popover.borrow_mut().take() {
                // `PersistentPopover::teardown()` releases any seat grab BEFORE
                // unrealizing the surface that holds it: it is `popdown()` THEN
                // `unparent()`, the one order that is safe here (ScrAP-144). `unparent()`
                // unrealizes the surface; doing that while a grab is live strands it —
                // and a stranded grab is not a dead app: button/key events keep routing
                // to the destroyed holder while motion/crossing still reach widgets, so
                // it looks like anything but a grab. The unparent is also mandatory
                // (GTK4Rs/AP-80 — a `set_parent`'d popover left parented at dispose leaks)
                // and must not migrate to a per-use destroy (GTK4Rs/AP-117). `teardown()`
                // encodes exactly this order, so the "stop unparenting" / "unparent while
                // open" wrong fixes are unrepresentable through the handle.
                p.teardown();
            }
            // The annotation card carries the same popdown→unparent order inside its own
            // `teardown`, plus the handlers it planted on objects that OUTLIVE the view
            // (the scroller's adjustment, for scroll-tracking; the toplevel, for
            // outside-click dismissal) — those must come off here or every render's card
            // would leave one behind on a longer-lived object (GTK4Rs/AP-28 discipline).
            if let Some(card) = self.marker_card.borrow_mut().take() {
                card.teardown();
            }
            // Drop our handler on the long-lived primary clipboard (table-cell annotation) so it
            // doesn't outlive the view and accumulate across renders (GTK4Rs/AP-28).
            if let Some((clip, id)) = self.primary_sel_handler.borrow_mut().take() {
                clip.disconnect(id);
            }
        }
    }

    impl WidgetImpl for CodePreviewView {
        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            // Re-bound the anchored block children BEFORE chaining up — using the
            // INCOMING allocation `width`, not the post-chain-up hadjustment page_size
            // (researcher-verified, gtk-4-6). Why the order is load-bearing:
            // `gtk_widget_size_allocate` runs this vfunc, then UNCONDITIONALLY clears
            // `alloc_needed_on_child` (gtkwidget.c:4095-96); so a `queue_resize` issued
            // AFTER `parent_size_allocate` is dropped this pass → the table is left
            // `alloc_needed` → the next snapshot bails ("snapshot ScribTableWidget
            // without a current allocation"). Issuing it BEFORE chain-up means
            // `gtk_text_view_allocate_children` re-validates the now-dirty child IN
            // THIS pass (gtktextview.c:4431-4447), so it exits correctly allocated.
            //
            // The preview has no gutter, so the text-window content column is
            // deterministic from the allocation: `width - margins`. We then subtract
            // the 1px SPACE_FOR_CURSOR GtkTextView adds to the line layout width (an
            // anchored child's advance can't be trimmed like wrapped text, so a child
            // sized to exactly the column makes the view 1px over-wide → spurious
            // Automatic h-bar that churns → blank, GTK4Rs/AP-23). Bounds every anchored block
            // child (rules AND tables) so none ever measures wider than the viewport.
            let view = self.obj();
            let content = width - view.left_margin() - view.right_margin();
            // Idempotent guard (MANDATORY): each set_width_request / set_bound_width
            // queue_resizes; re-issuing every frame would thrash. Only on a real change.
            if content > 0 && content != self.last_content_width.get() {
                self.last_content_width.set(content);
                let bound = content - ANCHORED_LINE_END_SLACK;
                for (w, inset) in self.width_bounded.borrow().iter() {
                    let target = (bound - inset).max(1);
                    if w.width_request() != target {
                        w.set_width_request(target);
                    }
                }
                for t in self.tables.borrow().iter() {
                    t.set_bound_width(bound);
                }
                // Images CLAMP to the viewport (not fill): keep natural size when it
                // fits, shrink to `bound` when too wide, height following by aspect
                // ratio so it never forces an over-wide line → GTK4Rs/AP-22/23 blank.
                for (w, max_w, max_h) in self.image_bounded.borrow().iter() {
                    if *max_w <= 0 {
                        continue;
                    }
                    let tw = (*max_w).min(bound).max(1);
                    let th = (i64::from(*max_h) * i64::from(tw) / i64::from(*max_w)).max(1) as i32;
                    if w.width_request() != tw || w.height_request() != th {
                        w.set_width_request(tw);
                        w.set_height_request(th);
                    }
                }
            }
            self.parent_size_allocate(width, height, baseline);

            // Reading-Position Preservation CAM (row 7 — geometry change). A
            // horizontal resize changes the text-wrap width, so the preview re-wraps
            // and GtkTextView re-validates line heights lazily; during that
            // re-validation a transiently smaller `upper` clamps the vadjustment
            // `value` toward 0 and the viewport JUMPS to the top — unless we re-anchor
            // to the user's tracked reading line (GTK4Rs/AP-13/65 family). Unlike a
            // buffer-swap event the buffer is unchanged here, so the reading line is
            // still valid and already captured continuously by
            // `wire_scroll_position_tracking`; we only restore it.
            //
            // Key strictly on the raw ALLOCATION `width`, never `content`: zoom changes
            // the margins (hence content) at the SAME width and owns its own restore
            // (`rerender_and_restore_scroll`), so re-anchoring on a margin change would
            // double-drive it. Skip the first real allocation (`prev < 0`): there is no
            // prior reading line to preserve and it must not fight the initial fresh
            // render's own restore. `reanchor_to_reading_line` only SCHEDULES a
            // coalesced, deferred, validation-safe scroll — no synchronous validation
            // from the size-allocate path (GTK4Rs/AP-22/29).
            let prev_w = self.last_alloc_width.get();
            if width != prev_w {
                self.last_alloc_width.set(width);
                if prev_w > 0 && width > 0 {
                    view.reanchor_to_reading_line();
                }
            }
        }

        /// Cancel any pending deferred-scroll idle before this view leaves the tree
        /// (ScrAP-152 / GTK4Rs/AP-63). `window.destroy()` unrealizes the subtree
        /// SYNCHRONOUSLY (gtkwindow.c:6717-6744), which runs BEFORE the pending
        /// `DEFAULT_IDLE`-priority scroll idle can dispatch — so cancelling here
        /// deterministically beats the idle and prevents it firing `scroll_to_mark`
        /// against a torn-down view (the measured SIGSEGV). `gtk_widget_dispose` also
        /// calls `unrealize` (gtkwidget.c:7418-7419), so this one hook covers both the
        /// explicit-destroy and refcount-0 teardown paths — and it is the RIGHT hook
        /// (unlike `dispose`, which can never run while a strong-capturing idle forms a
        /// reference cycle; the idles capture weakly precisely so that is moot here).
        ///
        /// Idempotent by construction: `unrealize` fires on every realize/unrealize
        /// cycle, and glib 0.21.5 `SourceId::remove()` PANICS on an already-fired id, so
        /// `take()` (not a peek) leaves a fired-and-cleared slot (None) a no-op and only
        /// removes a still-pending source. The closures clear the slot to None the
        /// instant they fire, upholding the "slot == Some(id) ⟺ source pending"
        /// invariant this relies on.
        fn unrealize(&self) {
            if let Some(id) = self.scroll_idle.borrow_mut().take() {
                id.remove();
            }
            self.parent_unrealize();
        }
    }

    impl CodePreviewView {
        /// Has the converge-scroll actually landed on `anchor`'s line?
        ///
        /// Recomputes the very aim `converge_and_scroll_to_offset` applies —
        /// `clamp(line_yrange(anchor).y, lower, upper - page_size)` — and asks whether
        /// the adjustment is there yet. `upper` grows as lazy line-height validation
        /// proceeds, so a `true` here means "as far as this document currently knows,
        /// the target is where it belongs", which is exactly the condition for opening
        /// a popover against it.
        ///
        /// The 4.0 px tolerance matches the navigation test's, and absorbs the
        /// sub-pixel drift between an animated scroll's final step and the aim.
        fn scroll_has_landed_on(&self, anchor: i32) -> bool {
            let view = self.obj();
            let Some(vadj) = view.vadjustment() else {
                // No adjustment means nothing to wait for — never block the dispatch on
                // a view that cannot scroll.
                return true;
            };
            let it = view.buffer().iter_at_offset(anchor);
            let (y, _h) = view.line_yrange(&it);
            let max = (vadj.upper() - vadj.page_size()).max(vadj.lower());
            let want = (y as f64).clamp(vadj.lower(), max);
            (vadj.value() - want).abs() < 4.0
        }
    }

    impl TextViewImpl for CodePreviewView {
        fn snapshot_layer(&self, layer: gtk::TextViewLayer, snapshot: gtk::Snapshot) {
            // Paint each code block's background in the BELOW-TEXT layer: GTK calls
            // this after the widget's own (opaque) CSS background but before the
            // text + selection, so the fill is visible under the text. (Drawing in
            // WidgetImpl::snapshot before parent_snapshot does NOT work — the widget
            // background paints over it. GTK4Rs/AP-21.)
            // Backgrounds (code blocks, blockquotes) paint in BELOW-TEXT (under the text
            // and under anchored children). Comment markers paint in ABOVE-TEXT so the
            // right-margin chip is drawn ON TOP of an anchored table widget — a cell
            // annotation's chip sits at the cell's buffer-Y, which is INSIDE the table's
            // vertical span, and BelowText would paint it behind the opaque table → invisible
            // (that was the "cell markers don't show" bug).
            if layer != gtk::TextViewLayer::BelowText && layer != gtk::TextViewLayer::AboveText {
                return;
            }
            let view = self.obj();
            let blocks = self.blocks.borrow();
            let blockquotes = self.blockquotes.borrow();
            let markers = self.markers.borrow();
            let list_markers = self.list_markers.borrow();
            if blocks.is_empty()
                && blockquotes.is_empty()
                && markers.is_empty()
                && list_markers.is_empty()
            {
                return;
            }
            let buffer = view.buffer();

            // The snapshot_layer paints in BUFFER coordinates (already scroll-
            // translated), so the visible region and every measurement below are in
            // that same space — no window-coordinate math.
            // First/last VISIBLE lines, clamped to [vis_start, vis_end]. Two hazards
            // shape the geometry reads below: (GTK4Rs/AP-22) an OFF-SCREEN validating read
            // (mid-draw validation → alloc_needed → blanked view/scrollbar) — avoided
            // by clamping the rect to the viewport; and (ScrAP-105) `iter_location`'s
            // line-display CACHE insert dereferences freed lines when a paint lands
            // right after a `set_buffer` swap — avoided by using `line_yrange` (a
            // cache-free btree-height read) for every extent, never `iter_location`.
            // The seam's `line_at_y` only maps a y to a line (no display cache), safe.
            let ViewportRange {
                top_y: vtop,
                bottom_y: vbot,
                top: top_iter,
                bottom: mut bot_iter,
            } = ViewportRange::of(&*view);
            let vis_start = top_iter.offset();
            if !bot_iter.ends_line() {
                bot_iter.forward_to_line_end();
            }
            let vis_end = bot_iter.offset();

            let bg = *self.bg.borrow();
            // Card aligns with the body-text column (the view's left margin); the
            // text is inset a further `pad` by the code-block tag, so the gap between
            // the card edge and the text is the inner padding.
            //
            // That inset is provided ENTIRELY by the code-block tags — horizontally
            // by `code-block`'s `left/right_margin` (= view margin + code_pad), and
            // VERTICALLY by `code-block-top`/`code-block-bottom`'s
            // `pixels_above/below_lines` (= code_pad), which expand the first/last
            // line's own `line_yrange`. The card is therefore drawn to exactly the
            // block's line-range extent `[line_top_y(start), line_bottom_y(last)]`
            // with NO extra vertical pad: the padding is already inside those lines.
            // Adding a further ±pad here double-counted it (24 px vertical vs the
            // 12 px horizontal) and — because that extra bottom pad reaches BEYOND
            // the block's last line — bled the card onto the immediately-following
            // line whenever no blank block-separator absorbed it. That happens for a
            // loose continuation paragraph abutting a code block inside a list item
            // (only ONE `\n` separates them, not a `block_sep` blank line), so the
            // paragraph's text rendered overlapping the card's bottom edge
            // (GTK4Rs/AP-127). Relying on the tags also keeps the padding zoom-correct:
            // `code_pad` is `px()`-scaled, the old raw `pad` was not.
            let lm = view.left_margin() as f32;
            let rm = view.right_margin() as f32;
            let card_w = (view.width() as f32 - lm - rm).max(0.0);

            if layer == gtk::TextViewLayer::BelowText {
                for &block in blocks.iter() {
                    if block.is_empty() {
                        continue;
                    }
                    // Skip blocks entirely off-screen.
                    if block.is_outside(vis_start, vis_end) {
                        continue;
                    }
                    // Clamp the RECT, never the iters: measure a boundary only when its
                    // line is on-screen (validated); a boundary that straddles the
                    // viewport edge clamps to that edge instead of reading an off-screen
                    // (unvalidated) iter. A block taller than the viewport clamps both
                    // ends and fills the visible height. The extent is the block's own
                    // line range with NO extra pad (GTK4Rs/AP-127 — see `span_card_y_extent`).
                    let (top, bottom) = span_card_y_extent(
                        &*view,
                        &buffer,
                        block,
                        vis_start,
                        vis_end,
                        vtop as f32,
                        vbot as f32,
                    );
                    if bottom > top {
                        let rect = graphene::Rect::new(lm, top, card_w, bottom - top);
                        snapshot.append_color(&bg, &rect);
                    }
                }

                // Blockquote accent bars — same visible-only, viewport-clamped Y-extent
                // logic as the code-block backgrounds (so we never read an off-screen,
                // unvalidated iter — GTK4Rs/AP-22), but drawn as a thin vertical rect at the
                // body-text left margin. Blockquotes are buffer text, so there is no
                // anchored widget here to re-measure/churn (GTK4Rs/AP-23).
                let bar_color = *self.bq_bar.borrow();
                // The bar's width is a themed decoration metric: a design-time px at
                // zoom 1.0, scaled here through the same `round(n * zoom)` the
                // `blockquote` tag scales its indent by (`tags.rs`). Scaling it is a
                // deliberate correction — the indent already scaled while the bar did
                // not, so a zoomed-in quote drew a hairline bar in a wide gutter. At
                // zoom 1.0 this is byte-identical to the previous constant.
                let bqm = crate::theme::active();
                let zoom_now = self.gutter_zoom.get();
                let bar_w = (bqm.metrics.blockquote_bar_width as f64 * zoom_now).round() as f32;
                for &quote in blockquotes.iter() {
                    if quote.is_empty() {
                        continue;
                    }
                    if quote.is_outside(vis_start, vis_end) {
                        continue;
                    }
                    let (top, bottom) = span_card_y_extent(
                        &*view,
                        &buffer,
                        quote,
                        vis_start,
                        vis_end,
                        vtop as f32,
                        vbot as f32,
                    );
                    if bottom > top {
                        let rect = graphene::Rect::new(lm, top, bar_w, bottom - top);
                        snapshot.append_color(&bar_color, &rect);
                    }
                }

                // List-item gutter markers. A bullet dot /
                // right-aligned number / static checkbox per item, drawn in the band
                // LEFT of the item's content margin, aligned to the item's FIRST line.
                // Same discipline as the bars above: paint VISIBLE first-lines only, so
                // the y read (`line_yrange`, cache-free — never `iter_location`, GTK4Rs/AP-22/
                // ScrAP-105; research §4) is on a validated line; x derives from `depth`
                // (`list_content_margin_px` == the `li-{depth}` content margin), never
                // from GTK geometry. Buffer-space, so scroll-correct for free.
                if !list_markers.is_empty() {
                    let zoom = self.gutter_zoom.get();
                    // The container text margins a list item's own indent accumulates
                    // ONTO, mirroring `tags.rs` exactly: the view's configured
                    // `left_margin` for a body item, and the `blockquote` tag's
                    // `left_margin + px(bar_width + text_gap)` for a quoted one.
                    // Both are SET properties (same read the bq bar above does), not
                    // lazily-validated layout — no GTK4Rs/AP-22 exposure. Without the quoted
                    // base, a quoted list's markers land left of the quote's accent bar
                    // (POLICY Document Rendering CAM row 2).
                    let body_base = lm;
                    // The SAME two theme keys `tags.rs` builds the `blockquote` tag's
                    // margin from, so a themed bar/gap can never leave a quoted list's
                    // markers beside the quote instead of inside it (GTK4Rs/AP-96).
                    let qm = crate::theme::active();
                    let quoted_base = lm
                        + ((qm.metrics.blockquote_bar_width + qm.metrics.blockquote_text_gap)
                            as f64
                            * zoom)
                            .round() as f32;
                    // Marker glyph ink: the active theme's `list_marker` if it sets one,
                    // else the widget foreground (the pre-theming default — keeps System
                    // byte-identical). Colours the bullet/numeral/checkbox only; the item
                    // text is buffer content and unaffected.
                    let fg = qm
                        .list_marker
                        .unwrap_or_else(|| view.style_context().color());
                    // Accent colour for a hovered checkbox's border — a named theme CSS
                    // colour via the style context (NO libadwaita; research §7). Distinct
                    // from the resting foreground, so the hover border reads as a change.
                    let hover_fg = view
                        .style_context()
                        .lookup_color("theme_selected_bg_color")
                        .or_else(|| view.style_context().lookup_color("accent_bg_color"))
                        .unwrap_or_else(|| gdk::RGBA::new(0.208, 0.518, 0.894, 1.0));
                    let hovered = self.hovered_checkbox.get();
                    // Rebuild the checkbox hit-boxes for the visible task items this paint
                    // (same clear+repopulate discipline as `marker_hitboxes`).
                    let mut cbhits: Vec<(graphene::Rect, usize)> = Vec::new();
                    // `line_yrange` returns the height of the WHOLE logical line, which for
                    // a soft-wrapped item spans EVERY display row — centering a marker on
                    // that whole span floats it to the MIDDLE row instead of the first
                    // (operator report 2026-07-22, most visible at higher zoom, which grows
                    // the rows and provokes the wrap). Clamp each item's height to its first
                    // display row via `first_display_line`. `single_line_h` is one row's
                    // text height in the view's OWN CSS-zoomed font — a fresh Pango layout
                    // (cache-free, never `iter_location`: GTK4Rs/AP-22), the same font the ordered
                    // numeral is drawn in, so it tracks zoom automatically. `marker_gap` is
                    // the item's `pixels_above_lines`, the band the text sits below.
                    let single_line_h = view.create_pango_layout(Some("0")).pixel_size().1 as f32;
                    let marker_gap = marker_gap_px(&qm.metrics, zoom);
                    for (idx, m) in list_markers.iter().enumerate() {
                        // The item's first line must be within the on-screen span —
                        // its `line_yrange` is 0/stale on an unvalidated line (research
                        // §4). Offset gate mirrors the code-block/blockquote loops.
                        if m.first_line < vis_start || m.first_line > vis_end {
                            continue;
                        }
                        let (y, h) = view.line_yrange(&buffer.iter_at_offset(m.first_line));
                        // Clamp the whole-logical-line height to the item's FIRST display
                        // row so the marker stays top-aligned when the item soft-wraps
                        // (see the `single_line_h` note above). A single-row item is
                        // unchanged. Both the drawn marker and the checkbox hit-column below
                        // derive from this clamped `(y, h)`, so they stay in lock-step.
                        let (y, h) =
                            first_display_line((y as f32, h as f32), single_line_h, marker_gap);
                        // Straddle clamp: skip a first-line whose refined y left the
                        // viewport (same guard the comment chips use).
                        if y + h < vtop as f32 || y > vbot as f32 {
                            continue;
                        }
                        let base = if m.quoted { quoted_base } else { body_base };
                        let content = list_content_margin_px(base, m.depth, zoom, &qm.metrics);
                        // Only TASK checkboxes are interactive: record a generous hit-box for
                        // the checkbox in BUFFER coordinates — the exact space the marker is
                        // drawn in. `is_over_checkbox` converts the incoming widget-space
                        // click back to buffer coords with GTK's own `window_to_buffer_coords`
                        // (the precise inverse of the draw transform), so there is NO
                        // hand-rolled scroll/margin math to drift — the earlier `- vtop`
                        // version silently displaced the zone by the margin on real
                        // compositors (invisible under Xvfb). The hit-box is the whole marker
                        // COLUMN for this item — from the parent depth's content margin across
                        // to this item's, and the full first-line height — so the small drawn
                        // box is not a pixel-hunt: clicking anywhere in the checkbox's gutter
                        // cell toggles it. Clamped to the item's own line so adjacent items'
                        // columns never overlap.
                        if matches!(m.kind, crate::renderer::ListMarkerKind::Task { .. }) {
                            let col_left = list_content_margin_px(
                                base,
                                m.depth.saturating_sub(1),
                                zoom,
                                &qm.metrics,
                            );
                            cbhits.push((
                                graphene::Rect::new(col_left, y, (content - col_left).max(0.0), h),
                                idx,
                            ));
                        }
                        draw_list_marker(
                            &snapshot,
                            view.upcast_ref::<gtk::TextView>(),
                            &m.kind,
                            content,
                            (y, h),
                            zoom,
                            &MarkerPaint {
                                fg: &fg,
                                hover: (hovered == Some(idx)).then_some(&hover_fg),
                                metrics: &qm.metrics,
                            },
                        );
                    }
                    *self.checkbox_hitboxes.borrow_mut() = cbhits;
                }
            } // end BelowText

            if layer == gtk::TextViewLayer::AboveText {
                // CriticMarkup comment markers: a small amber chip in
                // the reserved right margin at each annotated line. When several
                // annotations share one visual line they collapse to a single chip
                // showing a count. Visible-only measurement, viewport-anchored — never
                // reads an off-screen (unvalidated) iter (GTK4Rs/AP-22), same as the bars above.
                let mut hitboxes: Vec<(graphene::Rect, Vec<usize>)> = Vec::new();
                if !markers.is_empty() {
                    let mut vis_markers: Vec<(usize, f32, f32)> = Vec::new();
                    for (i, m) in markers.iter().enumerate() {
                        let is_cell = m.cell_widget.is_some();
                        // Body markers: cheap anchor-offset visibility gate. A CELL
                        // marker's anchor is the table U+FFFC, which can scroll off-screen
                        // while the cell's own row is still visible (tall tables), so defer
                        // its culling to the refined-Y check below.
                        if !is_cell && (m.anchor < vis_start || m.anchor > vis_end) {
                            continue;
                        }
                        // Base Y (anchor line), refined to the exact table row for a CELL
                        // marker, via the SHARED `marker_row_y_h` — the same formula the
                        // annotation card anchors itself with, so the chip and the card
                        // that points at it can never drift apart (GTK4Rs/AP-78/GTK4Rs/AP-127).
                        // Recomputed EVERY frame: both halves are cheap and scroll-stable,
                        // so there's nothing to cache against (no flicker, no stale cache).
                        let (y, h) = marker_row_y_h(&*view, &buffer, m);
                        // Skip chips whose refined Y is fully outside the viewport
                        // (tall tables: a lower-row cell can leave the view while the
                        // table anchor line is still "visible" by buffer offset).
                        if y + h < vtop as f32 || y > vbot as f32 {
                            continue;
                        }
                        vis_markers.push((i, y, h));
                    }
                    let ys: Vec<i32> = vis_markers.iter().map(|&(_, y, _)| y as i32).collect();
                    let width = view.width() as f32;
                    let accent = gdk::RGBA::new(0.90, 0.62, 0.10, 0.95);
                    let ink = gdk::RGBA::new(1.0, 1.0, 1.0, 1.0);
                    for (_gy, local) in group_by_line(&ys) {
                        let (_, y, h) = vis_markers[local[0]];
                        // SHARED chip arithmetic (`chip_rect`) — the annotation card
                        // re-derives its anchor with the very same call, so the drawn chip
                        // and the card pointing at it cannot disagree (GTK4Rs/AP-78/GTK4Rs/AP-127).
                        let (chip_x, cy, marker_w, chip_h) = chip_rect(width, rm, y, h);
                        snapshot.append_color(
                            &accent,
                            &graphene::Rect::new(chip_x, cy, marker_w, chip_h),
                        );
                        if local.len() > 1 {
                            let layout = view.create_pango_layout(Some(&local.len().to_string()));
                            snapshot.save();
                            snapshot.translate(&graphene::Point::new(chip_x + 2.0, cy - 1.0));
                            snapshot.append_layout(&layout, &ink);
                            snapshot.restore();
                        }
                        // Hit-box in WIDGET coords. Convert the chip's buffer-y to widget-y
                        // with GTK's own transform (chip_x is already widget-space). Using
                        // `cy - visible_rect().y()` drifts by the top margin on compositors
                        // where the two disagree — the same bug that displaced the task
                        // checkbox hit zone (operator, 2026-07-16); GTK4Rs/AP-80-safe (pure math).
                        let (_, chip_wy) =
                            view.buffer_to_window_coords(gtk::TextWindowType::Widget, 0, cy as i32);
                        let ann_idxs: Vec<usize> =
                            local.iter().map(|&li| vis_markers[li].0).collect();
                        hitboxes.push((
                            graphene::Rect::new(chip_x, chip_wy as f32, marker_w, chip_h),
                            ann_idxs,
                        ));
                    }
                }
                *self.marker_hitboxes.borrow_mut() = hitboxes;

                // Fire an armed open-request now that the hit-boxes are live.
                // THIS is the completion event a programmatic navigation was waiting for —
                // see `pending_marker_open` for why we must generate it ourselves rather
                // than wait on a GTK signal.
                //
                // Dispatch on an IDLE, NEVER inline: we are inside the draw path, and
                // `open_marker_popover` calls `popup()`, which re-enters layout/validation
                // (GTK4Rs/AP-22 — forcing layout from snapshot leaves the view stuck blank) and
                // rebuilds widgets mid-emission (GTK4Rs/AP-30 — "broken accounting of active
                // state"). The idle runs after this frame is on screen.
                let dispatch = {
                    let mut pending = self.pending_marker_open.borrow_mut();
                    match pending.as_ref() {
                        None => None,
                        // Expired — abandon, and check this BEFORE the hit-box: a paint
                        // arriving after the wall-clock budget is by construction not the
                        // paint this navigation caused, so opening now would throw a
                        // surprise popover at a rect the user is no longer asking about
                        // (they have scrolled elsewhere; the chip merely happens to be on
                        // screen again). Expiry wins over a late hit. The document keeps
                        // whatever position the converge loop left it at — the visible
                        // give-up.
                        Some(p) if p.deadline.expired() => {
                            *pending = None;
                            None
                        }
                        // Painted and in view — but NOT necessarily arrived. The chip can
                        // become visible while lazy line-height validation is still growing
                        // the adjustment's `upper`, so a dispatch here would freeze the
                        // scroll at whatever partial position happened to reveal it: the
                        // converge tick sees its request gone and stops re-aiming (correctly
                        // — the GTK4Rs/AP-118 re-pin guard owns the adjustment from that moment),
                        // and nothing corrects it afterwards. Measured on GDK-Win32: left at
                        // 103 against a reachable 263, because the chip surfaces earlier there
                        // relative to validation.
                        //
                        // So require the scroll to have LANDED, testing it directly against
                        // the same aim the converge loop computes. Deliberately a local test
                        // and not a "has the loop converged?" flag (ScrAP-202, which also
                        // records the half-fix this replaced: gating on convergence alone
                        // silenced the very paint the dispatch rides on, because the final
                        // `set_value` of a converged loop writes the value already held and
                        // so queues no draw): convergence is only
                        // observable through further frame-clock ticks, and those are not
                        // guaranteed — under a non-blocking pump the clock can go idle after
                        // a single tick, leaving a convergence gate that never opens and a
                        // request that never dispatches (measured on 4.6.9/X11, where exactly
                        // one paint is available). This test needs no extra frame: it is true
                        // or false about the state already in front of us, and while it is
                        // false the request simply stays armed and the loop keeps aiming.
                        //
                        // The clamp matters: where the target cannot reach the top of the
                        // viewport, the correct landing IS the end of the document, and
                        // `want` collapses to `max` — so this stays satisfiable rather than
                        // deadlocking on an unreachable goal. The deadline still bounds it.
                        Some(p) if !self.scroll_has_landed_on(p.anchor) => None,
                        Some(p) => {
                            // The hit-box is consulted as PROOF THAT THE CHIP PAINTED —
                            // the completion event this request has been waiting for — and
                            // for nothing else. Its RECTANGLE is deliberately not carried
                            // forward: a widget rect is only meaningful at the scroll
                            // offset it was read at, and handing one across the idle below
                            // is precisely how the card came to be positioned where the
                            // chip *had been* before the converge-scroll. The
                            // card re-derives its own anchor when it is presented.
                            let hit = self
                                .marker_hitboxes
                                .borrow()
                                .iter()
                                .find(|(_, idxs)| idxs.contains(&p.target))
                                .map(|(_, idxs)| idxs.clone());
                            if hit.is_some() {
                                *pending = None; // satisfied
                            }
                            hit
                        }
                    }
                };
                if let Some(idxs) = dispatch {
                    // WEAK capture, not a strong clone (ScrAP-152/GTK4Rs/AP-128/GTK4Rs/AP-63): a
                    // strong clone would pin this view alive as an unrooted zombie if its
                    // window is destroyed between this paint and the idle firing, and the
                    // idle would then drive `open_marker_popover` → `popup()` a popover on
                    // an unrealized view (NULL parent surface → GDK_IS_SURFACE SIGSEGV).
                    // De-pinned here, and `open_marker_popover` self-guards on
                    // `is_realized()` — defense-in-depth, either alone prevents the crash.
                    //
                    // The idle also happens to be the POST-PAINT read this path requires.
                    // We are inside the paint of the frame whose `size_allocate` already
                    // ran `flush_first_validate`, so by the time the idle runs the geometry
                    // the card is about to read is validated. That matters because the
                    // programmatic navigation is the one case where a pre-paint read is
                    // stably WRONG rather than merely late: a tick callback fires at the
                    // UPDATE phase, before LAYOUT, so it samples the same pre-validation
                    // estimate every frame and a stability check converges on it
                    // (GTK4Rs/AP-142/GTK4Rs/AP-142). Reading after the paint is the fix, and
                    // dispatching from inside the paint is how we get it for free.
                    let obj = self.obj();
                    glib::idle_add_local_once(glib::clone!(
                        #[weak(rename_to = view)]
                        obj,
                        move || view.open_marker_popover(&idxs)
                    ));
                }
            } // end AboveText
        }
    }
}

glib::wrapper! {
    pub(crate) struct CodePreviewView(ObjectSubclass<imp::CodePreviewView>)
        @extends gtk::TextView, gtk::Widget,
        @implements gtk::Scrollable, gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl CodePreviewView {
    /// Note that this view's content has just been (re-)rendered, and return the new
    /// generation. State derived from the rendered content keys on this — see
    /// `imp::CodePreviewView::render_generation` for why buffer identity cannot.
    pub(crate) fn bump_render_generation(&self) -> u64 {
        use gtk::subclass::prelude::*;
        let next = self.imp().render_generation.get().wrapping_add(1);
        self.imp().render_generation.set(next);
        next
    }

    /// The current render generation (see [`Self::bump_render_generation`]).
    pub(crate) fn render_generation(&self) -> u64 {
        use gtk::subclass::prelude::*;
        self.imp().render_generation.get()
    }

    pub(crate) fn new() -> Self {
        use gtk::subclass::prelude::*;
        let obj: Self = glib::Object::new();
        // Ctrl+Home / Ctrl+End. Read-only does not exempt this view: GTK's
        // buffer-ends key bindings are on GtkTextView, not on editability, and a
        // rendered document is exactly the kind that is still being laid out when
        // the key arrives (ScrAP-260). Wired here, at the one place a preview view is
        // constructed, so it cannot be missed by a later render path.
        crate::farscroll::wire_buffer_ends_scroll(obj.upcast_ref());
        // …and keep those bindings reachable when an anchored table cell holds the
        // focus: a selectable cell label consumes the horizontal and buffer-ends keys
        // with its own bindings, so they never reach the view and the document does
        // not move (ScrAP-264). Wired at the same one place, for the same reason.
        navkeys::wire_document_navigation_keys(&obj);
        // A genuine user scroll supersedes any cached programmatic reading anchor,
        // so a subsequent zoom re-captures the live position rather than snapping
        // back to the last zoom-restore/outline target.
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
        scroll.connect_scroll(glib::clone!(
            #[weak(rename_to = o)]
            obj,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, _, _| {
                // The user is now driving: subsequent `value-changed`s record the
                // settled top line (see `wire_scroll_position_tracking`), so a later
                // zoom re-anchors to where they left off.
                o.imp().user_scrolling.set(true);
                glib::Propagation::Proceed
            }
        ));
        obj.add_controller(scroll);

        // Left-click on a right-margin comment marker opens its popover. The
        // handler only acts on a click inside a recorded marker hit-box (all in the
        // right margin, where there is no text), so ordinary text selection is
        // untouched.
        let click = gtk::GestureClick::new();
        click.set_button(1);
        // Capture phase so the margin click is seen BEFORE the text view's own
        // selection gesture (which would otherwise claim the sequence and swallow
        // the release). We only claim it when it actually hits a marker.
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        // A COMPLETE click, press and release on the same marker: a selection drag that
        // runs off the end of a line into the right margin ends over a chip, and on the
        // release alone that opened the card the reader never clicked (GTK4Rs/AP-169). The
        // claim stays on the release, as before — a press that only *might* become a
        // marker click must not take the sequence away from the view's selection.
        crate::saferizer::ClickActivation::new().wire(
            &click,
            glib::clone!(
                #[weak(rename_to = o)]
                obj,
                #[upgrade_or]
                None,
                move |x, y| o.marker_group_at(x, y)
            ),
            glib::clone!(
                #[weak(rename_to = o)]
                obj,
                move |gesture: &gtk::GestureClick, idxs: Vec<usize>, _, _| {
                    o.open_marker_popover(&idxs);
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                }
            ),
        );
        obj.add_controller(click);

        // Bug B: pre-warm the marker popover on the view's FIRST map (deferred one idle
        // turn so the map completes first), while the pane is still at scroll 0 — this
        // absorbs the first-`set_parent`+`popup` view revalidation that otherwise scrolls
        // (and drops the click) on the first real marker activation. Self-guarded to run
        // once; the deferred call is a harmless no-op on later remaps (tab switch, etc.).
        let weak_map = obj.downgrade();
        obj.connect_map(move |_| {
            let weak = weak_map.clone();
            glib::idle_add_local_once(move || {
                if let Some(o) = weak.upgrade() {
                    o.prewarm_marker_popover();
                }
            });
        });
        obj
    }

    /// Connect the vadjustment `value-changed` that records the user's settled top
    /// line into `restore_target_line` while `user_scrolling`, and mark
    /// `user_scrolling` from EVERY hand-scroll input source — not just the wheel.
    /// Called once the view is inside its ScrolledWindow — from `preview::render`.
    ///
    /// The wheel `EventControllerScroll` (in `new`) alone is NOT enough: dragging
    /// the SCROLLBAR thumb (or clicking its trough) and KEYBOARD scrolling
    /// (PageUp/Down, arrows, Home/End) move the vadjustment WITHOUT emitting a
    /// scroll event, so `user_scrolling` stayed false, `value-changed` ignored the
    /// move, and `restore_target_line` kept a stale (near-top) line — a later zoom
    /// re-render or reload then re-anchored there instead of where the user left off
    /// (ScrAP-80; MANUAL-TEST 13.7 / 3.2). So hook the scrollbar and the
    /// scroll keys too. Setting `user_scrolling` liberally is safe for the
    /// rapid-zoom-burst path: `scroll_to_buffer_offset` resets it to
    /// false at the start of every programmatic scroll, so a burst's own animation
    /// `value-changed`s are still ignored and the cached target is never corrupted.
    pub(crate) fn wire_scroll_position_tracking(&self, scroller: &gtk::ScrolledWindow) {
        use gtk::subclass::prelude::*;
        scroller.vadjustment().connect_value_changed(glib::clone!(
            #[weak(rename_to = o)]
            self,
            move |adj| {
                if o.imp().user_scrolling.get() {
                    let (it, _) = o.line_at_y(adj.value() as i32);
                    o.imp().restore_target_line.set(Some(it.line()));
                }
            }
        ));
        // Scrollbar thumb-drag / trough-click: a press on the vertical scrollbar
        // means the user is driving. (BOTH buttons via button 0.)
        let bar = scroller.vscrollbar();
        let press = gtk::GestureClick::new();
        press.set_button(0);
        press.connect_pressed(glib::clone!(
            #[weak(rename_to = o)]
            self,
            move |_, _, _, _| {
                o.imp().user_scrolling.set(true);
            }
        ));
        bar.add_controller(press);
        // Keyboard scrolling of the (read-only) preview: PageUp/Down, arrows,
        // Home/End all move the vadjustment with no scroll event.
        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(glib::clone!(
            #[weak(rename_to = o)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, _| {
                use gtk::gdk::Key;
                if matches!(
                    key,
                    Key::Page_Up | Key::Page_Down | Key::Up | Key::Down | Key::Home | Key::End
                ) {
                    o.imp().user_scrolling.set(true);
                }
                glib::Propagation::Proceed
            }
        ));
        self.add_controller(keys);
    }

    /// The current top reading line, robust to a rapid re-render burst: the cached
    /// anchor (the user's settled top line, or a programmatic scroll's target) when
    /// set — never the transient, mid-animation adjustment value; the live top line
    /// only before any scroll. Used to re-anchor a zoom re-render so successive fast
    /// zooms hold position (ScrAP-65).
    pub(crate) fn reading_line(&self) -> i32 {
        use gtk::subclass::prelude::*;
        if let Some(l) = self.imp().restore_target_line.get() {
            return l;
        }
        // Through the seam, not a hand-rolled `vadjustment().value()` +
        // `line_at_y`: that pair is answerable only once the view has been
        // allocated, and before then `line_at_y` returns the buffer's LAST line
        // rather than declining (ScrAP-263). This is read on paths that run in the
        // same synchronous turn a view is built in — the cross-document
        // fragment-link arrival stamps a Back/Forward departure from it — so the
        // unallocated case is reached in production, not just in tests.
        crate::saferizer::viewport::ViewportTopIter::of(self).line()
    }

    /// Register this render's width-bounded anchored children — fixed-HEIGHT widgets
    /// (e.g. a horizontal-rule `GtkSeparator`) that just need a content-column width.
    /// Each `(widget, inset)`'s `width_request` is kept at `content - inset` as the
    /// view is allocated. (NOT for height-for-width content like tables — those churn
    /// the blank and use the custom `ScribTableWidget` via `set_tables`. GTK4Rs/AP-23.)
    pub(crate) fn set_width_bounded(&self, items: Vec<(gtk::Widget, i32)>) {
        use gtk::subclass::prelude::*;
        let imp = self.imp();
        imp.width_bounded.replace(items);
        imp.last_content_width.set(-1); // force a re-apply on the next allocation
        self.queue_allocate();
    }

    /// Register this render's anchored images — each `(picture, max_w, max_h)` is the
    /// image's natural size. `size_allocate` clamps each to the live content column
    /// (`w = min(max_w, content)`, height by aspect), so a too-wide image fits the
    /// viewport instead of blanking. CLAMP semantics, unlike `set_width_bounded`.
    pub(crate) fn set_image_bounded(&self, items: Vec<(gtk::Widget, i32, i32)>) {
        use gtk::subclass::prelude::*;
        let imp = self.imp();
        imp.image_bounded.replace(items);
        imp.last_content_width.set(-1); // force a re-apply on the next allocation
        self.queue_allocate();
    }

    /// Register this render's custom table widgets. `size_allocate` gives each its
    /// live content-column width via `set_bound_width`, which lays it out once per
    /// real width change and never re-measures its cells at validation (GTK4Rs/AP-23).
    pub(crate) fn set_tables(&self, tables: Vec<crate::widgets::table::ScribTableWidget>) {
        use gtk::subclass::prelude::*;
        let imp = self.imp();
        imp.tables.replace(tables);
        imp.last_content_width.set(-1); // force a re-apply on the next allocation
        self.queue_allocate();
    }

    /// Set the code blocks and fill color (called after a (re-)render), then
    /// repaint. Extents are measured live in `snapshot_layer`, so there is no
    /// cache to refresh — a redraw is all that's needed.
    pub(crate) fn set_code_blocks(&self, blocks: Vec<crate::span::BufferSpan>, bg: gdk::RGBA) {
        use gtk::subclass::prelude::*;
        let imp = self.imp();
        imp.blocks.replace(blocks);
        imp.bg.replace(bg);
        self.queue_draw();
    }

    /// Set the blockquote ranges and accent-bar colour (called after a (re-)render).
    /// The bars are drawn live in `snapshot_layer`, so a redraw is all that's needed.
    pub(crate) fn set_blockquotes(&self, ranges: Vec<crate::span::BufferSpan>, bar: gdk::RGBA) {
        use gtk::subclass::prelude::*;
        let imp = self.imp();
        imp.blockquotes.replace(ranges);
        imp.bq_bar.replace(bar);
        self.queue_draw();
    }

    /// Set this render's list-item markers and the `zoom` they were laid out at,
    /// then repaint. Positions are measured live in
    /// `snapshot_layer` (cache-free `line_yrange`), so a redraw is all that's needed —
    /// same as `set_blockquotes` / `set_code_blocks`. `zoom` is stored so the drawn
    /// marker x tracks the `li-{depth}` content margin (`px(depth*28)`) at any zoom.
    pub(crate) fn set_list_markers(&self, markers: Vec<crate::renderer::ListMarker>, zoom: f64) {
        use gtk::subclass::prelude::*;
        let imp = self.imp();
        imp.list_markers.replace(markers);
        imp.gutter_zoom.set(zoom);
        // A new marker set invalidates the old checkbox hit-boxes and any hover
        // identity (indices shift; a checkbox may be gone) — clear both so a stale
        // hover can never point at the wrong item. The next paint repopulates the
        // hit-boxes for the currently-visible task items.
        imp.checkbox_hitboxes.borrow_mut().clear();
        imp.hovered_checkbox.set(None);
        self.queue_draw();
    }

    /// The index (into `list_markers`) of the TASK checkbox whose recorded hit-box
    /// contains widget-coordinate `(x, y)`, or `None`. Mirrors [`Self::is_over_marker`]
    /// but returns the hovered item's identity so the motion handler can track it.
    /// Only task checkboxes are recorded — bullets/numbers
    /// never match.
    pub(crate) fn is_over_checkbox(&self, x: f32, y: f32) -> Option<usize> {
        use gtk::subclass::prelude::*;
        // The click arrives in WIDGET coords; the hit-boxes are stored in BUFFER coords
        // (where the markers are drawn). Convert with GTK's own inverse transform so no
        // hand-rolled scroll/margin math can drift (GTK4Rs/AP-80-safe — pure arithmetic, no
        // line-display validation, unlike `iter_at_location`).
        let (bx, by) =
            self.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
        let (bx, by) = (bx as f32, by as f32);
        self.imp()
            .checkbox_hitboxes
            .borrow()
            .iter()
            .find_map(|(r, idx)| {
                (bx >= r.x() && bx <= r.x() + r.width() && by >= r.y() && by <= r.y() + r.height())
                    .then_some(*idx)
            })
    }

    /// The CLEANED-source byte span of the `[ ]`/`[x]` marker for the task item at
    /// `idx` (into `list_markers`), or `None` when `idx` is out of range or that
    /// marker is not a `Task`. The checkbox-toggle gesture
    /// reads it for a hit checkbox, translates it to the editor's original source
    /// (`cleaned_to_original`), and routes a `ToggleTask` edit through the sink.
    pub(crate) fn task_marker_src(&self, idx: usize) -> Option<std::ops::Range<usize>> {
        use gtk::subclass::prelude::*;
        self.imp().list_markers.borrow().get(idx).and_then(|m| {
            if let crate::renderer::ListMarkerKind::Task { src, .. } = &m.kind {
                Some(src.clone())
            } else {
                None
            }
        })
    }

    /// Set which checkbox the pointer is over and repaint the hover border — but ONLY
    /// when the identity actually changes, so ordinary motion events don't thrash
    /// `queue_draw`. Passing `None` clears the hover (on
    /// leaving all checkboxes, or leaving the view entirely).
    pub(crate) fn set_hovered_checkbox(&self, idx: Option<usize>) {
        use gtk::subclass::prelude::*;
        if self.imp().hovered_checkbox.get() != idx {
            self.imp().hovered_checkbox.set(idx);
            self.queue_draw();
        }
    }
}

impl Default for CodePreviewView {
    fn default() -> Self {
        Self::new()
    }
}

/// GTK-object tests: constructing a `CodePreviewView` and driving a `GtkTextBuffer`
/// need an initialized GTK, so — like `window/reload.rs` — these use `#[gtktest::test]`
/// behind the `gtk-integration-tests` feature.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::geometry::span_card_y_extent;
    use super::*;

    /// TDD 13.7 / ScrAP-65 regression (area-1 automated test): a
    /// programmatic scroll RECORDS its target line as the cached reading anchor, so
    /// a following zoom re-render re-anchors to the intended reading position
    /// instead of drifting to the top. `reading_line()` must therefore report the
    /// LAST `scroll_to_buffer_offset` target — the exact value the zoom re-render
    /// reads back via `preview_top_line`. (The live visual drift is 13.7 in the
    /// manual plan; this pins the decision logic underneath it.)
    #[gtktest::test]
    fn programmatic_scroll_target_becomes_the_reading_anchor() {
        let view = CodePreviewView::new();
        // 200 short lines so distinct line offsets exist.
        let body: String = (0..200).map(|n| format!("line {n}\n")).collect();
        view.buffer().set_text(&body);

        // A programmatic scroll to line 120 must cache line 120 as the anchor.
        // (We don't assert the pre-scroll anchor: with no cached target it falls
        // back to `line_at_y` on an unmapped view, whose geometry isn't settled —
        // the cached-target path below is what the zoom re-render actually reads.)
        let offset = view.buffer().iter_at_line(120).map(|i| i.offset()).unwrap();
        view.scroll_to_buffer_offset(offset);
        assert_eq!(
            view.reading_line(),
            120,
            "reading_line must report the last programmatic scroll target (13.7 anchor)"
        );

        // A second programmatic scroll re-anchors to the newer target (a rapid
        // zoom burst re-targets, and the latest must win — never a stale line).
        let offset = view.buffer().iter_at_line(40).map(|i| i.offset()).unwrap();
        view.scroll_to_buffer_offset(offset);
        assert_eq!(
            view.reading_line(),
            40,
            "the newer scroll target supersedes the older"
        );
    }

    /// ScrAP-104 regression: a persisted scroll mark must never
    /// be resolved after a `set_buffer` orphaned it. `scroll_to_buffer_offset`
    /// schedules an idle that calls `scroll_to_mark(&mark)`; if a buffer swap
    /// finalizes the mark's owning buffer before the idle fires, resolving it would
    /// drive GTK 4.6's unchecked `get_iter_at_mark` into a garbage iter → the fatal
    /// `gtk_text_btree_line_number couldn't find line` abort (researcher-sourced from
    /// gtktextbuffer.c:2569 / gtktextbtree.c:3196). The cross-buffer guard bails on the
    /// orphaned mark. This drives the exact sequence; reaching the final assert proves
    /// the process survived the resolution (an unguarded build corrupts/aborts here).
    #[gtktest::test]
    fn a_scroll_mark_orphaned_by_set_buffer_is_never_resolved() {
        let view = CodePreviewView::new();
        let body_a: String = (0..200).map(|n| format!("alpha {n}\n")).collect();
        view.buffer().set_text(&body_a);

        // Schedule a deferred scroll_to_mark — creates the persistent mark on buffer A.
        let offset = view.buffer().iter_at_line(150).map(|i| i.offset()).unwrap();
        view.scroll_to_buffer_offset(offset);
        let mark_a = view
            .buffer()
            .mark("scrib-outline-target")
            .expect("scroll_to_buffer_offset installs the persistent mark on buffer A");
        assert!(!mark_a.is_deleted(), "the mark starts live on buffer A");

        // Swap the view's buffer BEFORE the idle fires. Nothing else holds buffer A,
        // so it finalizes here and orphans the mark — the exact cross-buffer condition.
        let buf_b = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
        buf_b.set_text("beta one\nbeta two\nbeta three\n");
        view.set_buffer(Some(&buf_b));

        // The researcher-sourced invariant the guard keys on (gtktextmark.c:327/352):
        // a mark whose owning buffer was finalized reports deleted / no buffer.
        assert!(
            mark_a.is_deleted(),
            "set_buffer finalized buffer A → its marks are deleted"
        );
        assert!(
            mark_a.buffer().is_none(),
            "an orphaned mark reports no owning buffer"
        );

        // Pump the main loop so the pending scroll idle fires under the guard. Reaching
        // the assert means scroll_to_mark was NOT called on the orphaned mark.
        let ctx = gtk::glib::MainContext::default();
        let mut spins = 0;
        while ctx.pending() && spins < 1000 {
            ctx.iteration(false);
            spins += 1;
        }
        // The guard is only exercised if the pending scroll idle ACTUALLY ran. With no
        // pending sources this loop spins zero times, the idle never fires, and the
        // test passes having driven none of the sequence it describes — the failure
        // mode is silence, not a red test.
        assert!(
            spins > 0,
            "no main-loop source was pending, so the deferred scroll idle never fired \
             and the cross-buffer guard was never reached"
        );
        assert!(
            !ctx.pending(),
            "the idle queue must have drained within {spins} iterations, or the guarded \
             idle may still be unrun"
        );
        // Reaching this line IS the result: an unguarded build resolves the orphaned
        // mark and aborts the process inside the loop above. The buffer equality below
        // is deliberately NOT the evidence — `set_buffer(Some(&buf_b))` twenty lines up
        // guarantees it, and it would hold just as well in a build that had aborted for
        // any other reason. It is kept only to pin that the no-op left the view alone.
        assert_eq!(
            view.buffer(),
            buf_b,
            "the guarded idle must have been a no-op, leaving the view on buffer B"
        );
    }

    /// ScrAP-105 regression: after a `set_buffer` swap warms then
    /// invalidates the line-display cache, resolving a geometry read must not touch the
    /// stale cache. `iter_location` (which inserts into the line-display GSequence,
    /// comparing against entries from the freed old buffer) SIGSEGVs here — that was the
    /// split scroll-sync crash. `line_yrange` (a pure cached btree-height read, no display
    /// cache) SURVIVES, which is why `project_scroll` uses it. Reaching the asserts proves
    /// we did not abort. (We deliberately do NOT call `iter_location` in this sequence —
    /// it aborts the whole test process, per the researcher's finding.)
    #[gtktest::test]
    fn line_yrange_survives_a_geometry_read_after_a_buffer_swap() {
        let view = CodePreviewView::new();
        // REALIZED, deliberately. An unrealized GtkTextView has no line-display cache
        // to warm and no layout to read: `iter_location` below is then a no-op and
        // `line_yrange` returns 0/0 for every iter, so the whole sequence would run
        // without ever establishing the stale-cache precondition it is named for, and
        // would pass on a build with the guard removed. Presenting the view is what
        // makes the warm-then-invalidate real.
        let win = gtk::Window::new();
        win.set_default_size(400, 300);
        win.set_child(Some(&view));
        win.present();
        let ctx = gtk::glib::MainContext::default();
        for _ in 0..200 {
            ctx.iteration(false);
            std::thread::sleep(std::time::Duration::from_millis(4));
            if view.is_mapped() && view.height() > 0 {
                break;
            }
        }
        assert!(
            view.is_mapped() && view.height() > 0,
            "the view must be realized, or there is no line-display cache to warm and \
             this test cannot reproduce ScrAP-105's precondition"
        );

        let a: String = (0..200).map(|n| format!("alpha {n}\n")).collect();
        view.buffer().set_text(&a);
        // Let A lay out, or there is no populated line-display cache for the swap to
        // invalidate and the sequence below is a swap between two cold buffers.
        for _ in 0..100 {
            ctx.iteration(false);
            std::thread::sleep(std::time::Duration::from_millis(4));
        }
        // Warm the layout / line-display cache against buffer A.
        let warm = view.iter_location(&view.buffer().iter_at_offset(500));
        // THE PRECONDITION, asserted rather than assumed: the warm must have produced
        // real geometry deep inside A. If it comes back at the origin, the cache was
        // never populated and everything after this tests a swap that had nothing
        // stale to trip over — the test would pass with the guard deleted.
        assert!(
            warm.y() > 0,
            "the line-display cache must actually be warm against buffer A before the \
             swap, or this test cannot reproduce ScrAP-105 (got y {})",
            warm.y()
        );
        // Swap to a shorter buffer B, finalizing A and freeing its lines.
        let b = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
        b.set_text("short one\nshort two\n");
        view.set_buffer(Some(&b));
        // The offset clamps into B; the geometry read must be cache-free (line_yrange).
        let it = view.buffer().iter_at_offset(500);
        // `offset() <= char_count()` cannot fail — `iter_at_offset` clamps by GTK
        // contract, so it asserts GTK's documented behaviour rather than anything
        // about this code. Assert the clamp ACTUALLY happened instead: offset 500 is
        // past the end of the short buffer B, so a correctly-clamped iter sits exactly
        // at its end. That distinguishes "clamped into B" from "still pointing into
        // the freed buffer A", which is the condition under test.
        assert_eq!(
            it.offset(),
            view.buffer().char_count(),
            "offset 500 is past the end of buffer B, so the iter must clamp to its end"
        );
        // The call under test. Surviving it IS the result — an `iter_location` here
        // SIGSEGVs against the freed buffer's cache entries, which was the split
        // scroll-sync crash. Note what is deliberately NOT asserted: the returned
        // yrange is 0/0 at this instant and correctly so, because B has not been
        // revalidated yet. Pumping the loop first to get a "usable" number would move
        // the read past the very window in which the stale cache is dangerous, i.e.
        // out of the condition under test. So the value is not the subject and is not
        // asserted on; the precondition above and the survival here are.
        let (y, _h) = view.line_yrange(&it);
        // No assertion on the raw pair: `y >= 0 && h >= 0` would be the same vacuous
        // shape this test was flagged for, since GTK does not return negative extents.
        // The one real statement available about the value is the residue check below.
        //
        // Residue check: whatever it answered, it must not be A's geometry. The warm
        // read above sat ~200 lines down; anything near that means the read was served
        // out of the freed buffer's cache.
        assert!(
            y < warm.y(),
            "the read must not answer out of freed buffer A's cache: got y {y} against \
             A's warmed y {}",
            warm.y()
        );
        win.destroy();
    }

    /// GTK4Rs/AP-127 regression: a code block's self-drawn card must not paint below its own
    /// last line. When a loose continuation paragraph abuts a code block inside a list
    /// item (only ONE `\n` between them — no `block_sep` blank line), the old `+pad`
    /// added to the card bottom reached 12 px BEYOND the code line's own `line_yrange`
    /// and overlapped the following paragraph's text. The card's bottom must be exactly
    /// `line_bottom_y(last_content)` — the very top of the next line — so it never
    /// covers it. (The 12 px vertical inner padding is supplied by the
    /// `code-block-top`/`code-block-bottom` tags, already inside the line's yrange.)
    #[gtktest::test]
    fn code_block_card_does_not_bleed_onto_the_following_line() {
        use gtk::subclass::prelude::*;
        // A fenced code block followed by a hard-break loose paragraph, inside a nested
        // ordered list item — the exact GTK4Rs/AP-127 construct (the `**OR**` line abuts the
        // code block with no blank separator).
        let md = "2. If you've configured git, otherwise either:\n   1. use our `.githooks`:\n      ```\n      git config set core.hookspath .githooks\n      ```\n      **OR**  \n   2. Add the `-s` flag when committing:\n      ```\n      git commit -s -m \"msg\"\n      ```\n";
        let pane = crate::preview::render(md, None, 1.0, false);
        let view = pane
            .clone()
            .downcast::<gtk::Overlay>()
            .unwrap()
            .child()
            .and_then(|c| c.downcast::<gtk::ScrolledWindow>().ok())
            .unwrap()
            .child()
            .and_then(|c| c.downcast::<CodePreviewView>().ok())
            .unwrap();
        let win = gtk::Window::new();
        win.set_default_size(800, 600);
        win.set_child(Some(&pane));
        win.present();

        let ctx = glib::MainContext::default();
        for _ in 0..200 {
            ctx.iteration(false);
            std::thread::sleep(std::time::Duration::from_millis(4));
            if view.is_mapped() && view.height() > 0 {
                break;
            }
        }
        assert!(view.is_mapped() && view.height() > 0, "view allocated");

        let buffer = view.buffer();
        // Reproduce snapshot_layer's exact visible-range setup so the card extent is
        // computed by the SAME formula the paint uses (span_card_y_extent) — not an
        // independent recomputation (which would be a tautology, GTK4Rs/AP-78).
        let ViewportRange {
            top_y,
            bottom_y,
            top: top_iter,
            bottom: mut bot_iter,
        } = ViewportRange::of(&view);
        let vtop = top_y as f32;
        let vbot = bottom_y as f32;
        let vis_start = top_iter.offset();
        if !bot_iter.ends_line() {
            bot_iter.forward_to_line_end();
        }
        let vis_end = bot_iter.offset();

        let blocks = view.imp().blocks.borrow().clone();
        assert_eq!(blocks.len(), 2, "the fixture has two code blocks");
        // The assertion below sits inside an `if let`, so a block with no following
        // line contributes NOTHING and the loop still passes — and the fixture's
        // second block is the last thing in the document, so that is not hypothetical.
        // Count the blocks actually checked and require at least one, or a fixture
        // edit that removed the abutting paragraph would leave this test green while
        // testing nothing (ScrAP-209).
        let mut checked = 0usize;
        for cb in &blocks {
            let (_card_top, card_bottom) =
                span_card_y_extent(&view, &buffer, *cb, vis_start, vis_end, vtop, vbot);
            // The line immediately AFTER the block starts here — the card must not paint
            // over its top (the GTK4Rs/AP-127 overlap was exactly the card reaching 12 px past
            // its own last line onto the abutting loose paragraph).
            let next_line = buffer.iter_at_offset(cb.last_content()).line() + 1;
            if let Some(next) = buffer.iter_at_line(next_line) {
                let next_top = view.line_yrange(&next).0 as f32;
                assert!(
                    card_bottom <= next_top,
                    "card bottom {card_bottom} must not reach past the next line's top {next_top}"
                );
                checked += 1;
            }
        }
        assert!(
            checked >= 1,
            "no code block in the fixture had a following line, so the overlap this \
             test guards was never evaluated — the GTK4Rs/AP-127 construct requires a block \
             ABUTTED by a following paragraph"
        );
        win.destroy();
    }
}
