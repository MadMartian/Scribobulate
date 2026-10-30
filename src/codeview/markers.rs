//! CriticMarkup annotation markers for [`CodePreviewView`]: the marker/edit data
//! types, and the view's marker state — the setters and accessors the window and
//! the create overlay install through, the card-session predicates, the chip rect
//! and the hit-tests. Split out of the former monolithic `codeview.rs` (F-AP-002);
//! the chip *painting* stays in the view's `snapshot_layer` vfunc (in `mod.rs`).
//!
//! The rest of the marker layer lives in siblings: the card's row building in
//! `markerrows`, programmatic navigation to a marker in `markernav`, and opening and
//! pre-warming the card in `markercard`.

use super::CodePreviewView;
use gtk::prelude::*;
use gtk::{gdk, glib, graphene};

/// The byte-range fields that map a marker back to its construct in the source,
/// grouped so [`MarkerData`] stays focused on display concerns (F-AP-001). Every
/// range is original-source byte coordinates EXCEPT `cleaned_content`, which is the
/// delimiter-free cleaned-document range used to pair a marker with its table cell.
/// This is the sub-group the mutation path (Edit/Remove) and the cell-pairing pass
/// (`preview::cells`) consume; the display fields on `MarkerData` don't touch it.
#[derive(Clone)]
pub(crate) struct MarkerSource {
    /// The full construct in the ORIGINAL source, anchored to its own text
    /// (drives Remove and Edit).
    ///
    /// An [`AnchoredSpan`](crate::docref::AnchoredSpan) rather than a bare
    /// range because a marker outlives the source it was scanned from: the card
    /// built from it can still be open after the user has typed, after an undo,
    /// or after the split-mode live re-render has re-scanned the document. Its
    /// `captured_at().start` remains the annotation's navigation identity.
    pub(crate) construct: crate::docref::AnchoredSpan,
    /// The highlighted content's byte range in the ORIGINAL source, kept when the
    /// annotation is removed; `None` for a point comment (whole construct goes).
    pub(crate) src_content: Option<std::ops::Range<usize>>,
    /// Comment-body byte range in the ORIGINAL source (drives Edit).
    pub(crate) src_comment_body: std::ops::Range<usize>,
    /// Cleaned-document range of the claim (or point-comment anchor) — used to
    /// pair a marker with a table-cell label after the widgets exist (cell-marker pairing).
    pub(crate) cleaned_content: std::ops::Range<usize>,
}

/// One CriticMarkup comment marker to draw in the preview's right margin,
/// Google-Docs / GitHub-PR style. Carries the buffer offset it anchors to
/// (for vertical placement) and everything the popover needs to display and
/// mutate the annotation.
///
/// **Do NOT "upgrade" this to an inline anchored `GtkTextChildAnchor` marker.**
/// That was tried and rejected: a claim boundary is almost always *mid* pulldown
/// text-event, so the anchor char (`U+FFFC`) would land at a mid-event buffer
/// position and **shift every buffer-offset-based feature after it** — copymap,
/// sourcemap (scroll-sync), link ranges, heading offsets, outline scroll-spy,
/// find/replace, image-tint selection. The margin marker inserts no buffer char
/// at all, so all those maps stay pristine. Accepted trade-off: a self-drawn
/// glyph is not in the a11y tree, so `win.next-annotation`
/// (`open_stepped_marker_popover`) is the keyboard read path.
#[derive(Clone)]
pub(crate) struct MarkerData {
    /// Buffer char offset the marker sits beside (end of a highlighted claim, or
    /// a point comment's position). Drives the marker's vertical position.
    pub(crate) anchor: i32,
    /// The comment text to show in the popover.
    pub(crate) comment: String,
    /// The highlighted claim snippet (for the popover header); `None` for a
    /// point comment.
    pub(crate) claim: Option<String>,
    /// Byte-range mapping back to the construct in the source (Edit/Remove and the
    /// table-cell pairing). Grouped into [`MarkerSource`] (F-AP-001).
    pub(crate) source: MarkerSource,
    /// When the claim lives in a table cell, a weak ref to that cell's `GtkLabel`
    /// so `snapshot_layer` can refine the chip Y to the cell's row (cell-marker
    /// pairing; table cells). `None` for body-text annotations. `snapshot_layer`
    /// translates this label into its `ScribTableWidget` ancestor (a scroll- and
    /// placeholder-immune local transform, bug C) — no cached Y is needed.
    pub(crate) cell_widget: Option<glib::WeakRef<gtk::Label>>,
    /// For a cell marker, the TABLE's own `GtkTextChildAnchor` — its
    /// `line_yrange` is the table-top buffer-Y the cell→table offset (`cy`) is
    /// added to (bug C). This is NOT `anchor`: `anchor` is a cleaned-offset proxy
    /// that can resolve a line or two ABOVE the table (its `line_yrange` was 36px
    /// too high in testing), whereas `iter_at_child_anchor(this)` lands exactly on
    /// the table's line. Set together with `cell_widget`; `None` for body markers.
    pub(crate) cell_table_anchor: Option<gtk::TextChildAnchor>,
}

/// An annotation mutation requested from a marker's popover (or the selection
/// overlay). The view emits this; the **window layer** performs it on the
/// document source buffer (the view never touches `editor_buf`).
#[derive(Clone, Debug)]
pub(crate) enum AnnotationEdit {
    /// Replace a comment body with new text.
    ///
    /// Anchored on the WHOLE construct, not on the comment body alone: a body is
    /// often a handful of characters ("typo", "?") that occur all over a document,
    /// so it carries far too little identity to re-locate on its own. `body` is
    /// therefore a range RELATIVE to the construct's start, re-absolutised once the
    /// construct has been resolved against the live source.
    EditComment {
        construct: crate::docref::AnchoredSpan,
        body: std::ops::Range<usize>,
        text: String,
    },
    /// Remove a construct, keeping the highlighted content (`keep_inner = Some`)
    /// or deleting it whole (a point comment, `None`).
    ///
    /// `keep_inner` is RELATIVE to the construct's start, for the same reason as
    /// `EditComment::body`.
    Remove {
        construct: crate::docref::AnchoredSpan,
        keep_inner: Option<std::ops::Range<usize>>,
    },
    /// Create a new annotation (from the preview selection overlay, Part C).
    Create(CreateAnnotation),
    /// Flip a task-list checkbox's `[ ]`↔`[x]` state character. `span` is the
    /// marker's byte range in the ORIGINAL
    /// source (the editor buffer's raw text — the render records it in CLEANED
    /// bytes; the preview gesture translates it via `cleaned_to_original` before
    /// building this). NOT a CriticMarkup construct: this variant rides the same
    /// preview→editor sink purely to reuse its "edit the editor source, deferred,
    /// one user-action, free dirty + re-render" contract. The enum has thus
    /// outgrown its `Annotation` name (it is really the set of preview-initiated
    /// edits to the editor source buffer — annotations were the first, the task
    /// toggle the second); renaming it across the tree is out of Phase-3b scope
    /// (noted, not fixed).
    ToggleTask { span: std::ops::Range<usize> },
}

/// A new annotation to insert (Part C — selection overlay).
///
/// **`target` is an [`AnchoredSpan`](crate::docref::AnchoredSpan), not a range**, for
/// the reason its siblings above are: the comment card is open while the reader types,
/// and the document can move underneath it — the split-mode re-render, an undo, a
/// Replace All, an external reload. A range captured when the card was raised is an
/// instruction about where to cut in a document that no longer exists; the text at that
/// range can be found again. Resolved at the one choke point the mutation is applied
/// (`window::annotate::apply_annotation_edit`).
#[derive(Clone, Debug)]
pub(crate) enum CreateAnnotation {
    /// Wrap the target as a highlight and attach `comment`.
    Highlight {
        target: crate::docref::AnchoredSpan,
        comment: String,
    },
    /// A point comment at the target's END (the cross-block fallback).
    Point {
        target: crate::docref::AnchoredSpan,
        comment: String,
    },
}

/// The sink the window installs to perform an [`AnnotationEdit`] on the document.
pub(crate) type AnnotationSink = std::rc::Rc<dyn Fn(AnnotationEdit)>;

/// A closure that reveals the preview's comment-entry card over the CURRENT preview
/// selection. The create overlay ([`crate::preview::annotate`]) stores it so the
/// `win.annotate` action can raise the very same card the selection popover's
/// **Annotate** button does — one code path behind both surfaces (SSOT).
pub(crate) type AnnotateTrigger = std::rc::Rc<dyn Fn()>;

impl CodePreviewView {
    /// Set this render's CriticMarkup comment markers and
    /// repaint. Positions are measured live in `snapshot_layer`, so a redraw is
    /// all that is needed.
    pub(crate) fn set_markers(&self, markers: Vec<MarkerData>) {
        use gtk::subclass::prelude::*;
        // Cell markers refine their row Y via a cell→table transform (bug C), which is
        // unavailable until the table has allocated its cells (incremental anchored-child
        // allocation). Nudge one more paint on idle — after the layout pass — so the chip
        // snaps from the table-top fallback to its exact row without needing a mouse-move.
        let has_cell_marker = markers.iter().any(|m| m.cell_widget.is_some());
        // An open card holds this render's marker indices and identity; re-key it onto the
        // new list, or dismiss it if its annotation is gone (`cardfollow`).
        let card_plan = super::cardfollow::plan(self, &markers);
        self.imp().markers.replace(markers);
        super::cardfollow::apply(self, card_plan);
        self.queue_draw();
        if has_cell_marker {
            gtk::glib::idle_add_local_once(glib::clone!(
                #[weak(rename_to = v)]
                self,
                move || {
                    v.queue_draw();
                }
            ));
        }
    }

    /// Install the sink the window uses to mutate the document source when a
    /// popover's Edit/Remove (or the selection overlay's Annotate) fires. Until
    /// set, popovers are read-only.
    pub(crate) fn set_annotation_sink(&self, sink: AnnotationSink) {
        use gtk::subclass::prelude::*;
        self.imp().annotation_sink.replace(Some(sink));
    }

    /// The installed annotation sink, if any (the create overlay reads it at Save).
    pub(crate) fn annotation_sink(&self) -> Option<AnnotationSink> {
        use gtk::subclass::prelude::*;
        self.imp().annotation_sink.borrow().clone()
    }

    /// Store the closure that raises the comment-entry card over the current
    /// selection (installed by the create overlay). Lets the `win.annotate` action
    /// share the popover Annotate button's exact code path.
    pub(crate) fn set_annotate_trigger(&self, trigger: AnnotateTrigger) {
        use gtk::subclass::prelude::*;
        self.imp().annotate_trigger.replace(Some(trigger));
    }

    /// Raise the comment-entry card over the current selection via the stored
    /// trigger. Returns `false` (a no-op) when no trigger is installed (e.g. before
    /// the create overlay wired this render) — the caller can then do nothing.
    pub(crate) fn trigger_annotate(&self) -> bool {
        use gtk::subclass::prelude::*;
        let trigger = self.imp().annotate_trigger.borrow().clone();
        if let Some(trigger) = trigger {
            trigger();
            true
        } else {
            false
        }
    }

    /// Record the overlay this view's pane floats its cards over. Set once, where the pane
    /// is built, before any card is shown.
    pub(crate) fn set_card_host(&self, host: &gtk::Overlay) {
        use gtk::subclass::prelude::*;
        self.imp().card_host.set(Some(host));
    }

    /// The overlay this view's pane floats its cards over, if the pane has one.
    pub(crate) fn card_host(&self) -> Option<gtk::Overlay> {
        use gtk::subclass::prelude::*;
        self.imp().card_host.upgrade()
    }

    /// Remember the selection action card so opening a marker card can close it.
    pub(crate) fn store_overlay_popover(&self, pop: &crate::widgets::floating_card::FloatingCard) {
        use gtk::subclass::prelude::*;
        self.imp().overlay_popover.replace(Some(pop.clone()));
    }

    /// Store the create overlay's primary-clipboard `changed` handler (table-cell annotation) so
    /// the view's `dispose` disconnects it from the long-lived display clipboard.
    /// Replaces any previous handler (disconnecting it first) so re-wiring never
    /// stacks handlers on the shared clipboard.
    pub(crate) fn store_primary_sel_handler(
        &self,
        clip: &gdk::Clipboard,
        id: glib::SignalHandlerId,
    ) {
        use gtk::subclass::prelude::*;
        if let Some((old_clip, old_id)) = self.imp().primary_sel_handler.borrow_mut().take() {
            old_clip.disconnect(old_id);
        }
        self.imp()
            .primary_sel_handler
            .replace(Some((clip.clone(), id)));
    }

    /// Whether the annotation card is currently showing an annotation. The
    /// create-selection overlay consults this so it doesn't re-show the Annotate popover
    /// over a table-cell selection that is still live while the user is reading/editing an
    /// existing marker (table-cell annotation — otherwise the two popovers stack, since a
    /// cell selection is not cleared by a margin-marker click).
    ///
    /// "Open" is the card's own lag-free flag, NOT slot presence (the card is a persistent
    /// instance, so the slot stays `Some` for the view's life) and NOT `is_visible()`
    /// — which now differs from "open" for a second reason too: the card hides
    /// itself while its chip is scrolled off the viewport, without ending its session.
    /// **No production caller by design** — the two visual-collision gates that used to
    /// call it now (correctly) ask [`marker_card_is_showing`] instead. It survives as the
    /// predicate the *teardown* tests need: they assert a reload ENDS the session, and
    /// asking "is it on screen" there would pass for a card that is merely hidden.
    ///
    /// [`marker_card_is_showing`]: Self::marker_card_is_showing
    #[cfg_attr(not(all(test, feature = "gtk-integration-tests")), allow(dead_code))]
    pub(crate) fn has_open_marker_popover(&self) -> bool {
        use gtk::subclass::prelude::*;
        self.imp()
            .marker_card
            .borrow()
            .as_ref()
            .is_some_and(|c| c.is_open())
    }

    /// Whether the annotation card is **on screen right now**.
    ///
    /// The predicate for "don't raise another surface over it" — distinct from
    /// [`has_open_marker_popover`], which answers the *session* question and stays true
    /// while the card is hidden because its chip scrolled off the viewport. Gating a
    /// visual-collision decision on the session would suppress the create-Annotate
    /// affordance against a card the user cannot see, with nothing on screen to explain
    /// why (a regression introduced the moment the card gained a hidden state, and the
    /// reason the two questions now have two answers).
    ///
    /// [`has_open_marker_popover`]: Self::has_open_marker_popover
    pub(crate) fn marker_card_is_showing(&self) -> bool {
        use gtk::subclass::prelude::*;
        self.imp()
            .marker_card
            .borrow()
            .as_ref()
            .is_some_and(|c| c.is_showing())
    }

    /// Dismiss the annotation card if one is showing.
    ///
    /// For callers about to destroy this view: the card is parented to it, so the view
    /// cannot be torn down out from under it (see `dispose`). `dispose` dismisses too, but
    /// a caller that KNOWS the preview is being replaced should say so here rather than
    /// rely on teardown ordering to rescue it.
    ///
    /// Clones the card OUT and drops the slot borrow before dismissing: the `closed`
    /// handler only touches a `Cell`, but holding a `RefCell` borrow across a signal
    /// emission is the GTK4Rs/AP-61 re-entrant-abort shape, and it costs nothing to not do it.
    /// Reads the slot directly rather than `marker_card()`, which would create a card just
    /// to close it.
    pub(crate) fn popdown_marker_popover(&self) {
        use gtk::subclass::prelude::*;
        let card = self.imp().marker_card.borrow().clone();
        if let Some(card) = card {
            card.dismiss();
        }
    }

    /// The chip rectangle for marker `index`, in this view's WIDGET coordinates,
    /// **re-derived from the marker's own anchor at the moment of the call**.
    ///
    /// This is the anti-stale-rect primitive. Nothing caches its result: a widget
    /// rectangle is only meaningful at the scroll offset it was read at, and every one of
    /// the card's three anchoring defects was a rectangle outliving its offset. It is called
    /// on the way
    /// through its `show` vfunc and again on every scroll, so it always points at where
    /// the chip *is*, never where it *was*.
    ///
    /// Shares [`marker_row_y_h`] and [`chip_rect`] with the painter, so the anchor and the
    /// drawn chip cannot drift apart. Every read here is cache-free and
    /// side-effect-free — `line_yrange` is a btree height read, and
    /// `buffer_to_window_coords` is a bare `y -= yoffset` — so this forces no layout
    /// validation and is safe to call from a scroll handler (GTK4Rs/AP-22). Returns `None` for an
    /// index this render's `markers` no longer has.
    pub(crate) fn marker_chip_rect(&self, index: usize) -> Option<gdk::Rectangle> {
        use gtk::subclass::prelude::*;
        let markers = self.imp().markers.borrow();
        let m = markers.get(index)?;
        let buffer = self.buffer();
        let (row_y, row_h) = crate::codeview::geometry::marker_row_y_h(self, &buffer, m);
        let (x, buf_y, w, h) = crate::codeview::geometry::chip_rect(
            self.width() as f32,
            self.right_margin() as f32,
            row_y,
            row_h,
        );
        let (_, widget_y) =
            self.buffer_to_window_coords(gtk::TextWindowType::Widget, 0, buf_y as i32);
        Some(gdk::Rectangle::new(x as i32, widget_y, w as i32, h as i32))
    }

    /// Whether widget-coordinate `(x, y)` is over a comment marker's hit-box — so
    /// the hover cursor can switch to a pointer, signalling the marker is clickable.
    pub(crate) fn is_over_marker(&self, x: f32, y: f32) -> bool {
        use gtk::subclass::prelude::*;
        self.imp().marker_hitboxes.borrow().iter().any(|(r, _)| {
            x >= r.x() && x <= r.x() + r.width() && y >= r.y() && y <= r.y() + r.height()
        })
    }

    /// The marker group whose hit-box contains `(x, y)` (widget coords), or `None`.
    ///
    /// Returns the group's annotation indices — the marker's *identity* — because that
    /// is what a complete click compares: `saferizer::ClickActivation` opens the
    /// popover only when a press and its release both land on the **same** marker, so a
    /// hit-test answering a bare `bool` (as this one's caller once did on release
    /// alone) cannot express the question (GTK4Rs/AP-169).
    ///
    /// The hit-box answers WHICH marker was clicked; it does not supply the card's
    /// anchor. The card re-derives that itself, so the rect stops here.
    pub(super) fn marker_group_at(&self, x: f64, y: f64) -> Option<Vec<usize>> {
        use gtk::subclass::prelude::*;
        let (px, py) = (x as f32, y as f32);
        self.imp()
            .marker_hitboxes
            .borrow()
            .iter()
            .find(|(r, _)| {
                px >= r.x() && px <= r.x() + r.width() && py >= r.y() && py <= r.y() + r.height()
            })
            .map(|(_, idxs)| idxs.clone())
    }

    /// The painted hit-box whose group contains marker `idx`, if it is on screen.
    /// Markers on one visual line share ONE grouped hit-box (`group_by_line`), so the
    /// lookup is by membership, not equality.
    pub(super) fn marker_hitbox_containing(
        &self,
        idx: usize,
    ) -> Option<(graphene::Rect, Vec<usize>)> {
        use gtk::subclass::prelude::*;
        self.imp()
            .marker_hitboxes
            .borrow()
            .iter()
            .find(|(_, idxs)| idxs.contains(&idx))
            .map(|(r, idxs)| (*r, idxs.clone()))
    }
}
