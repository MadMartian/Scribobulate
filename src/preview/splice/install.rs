//! Adopting a [`super::splice`] into a live preview pane: the merged products, the
//! region's own controls, and the reader's place.
//!
//! [`super`] owns producing a correct region write. This owns everything that has to
//! happen for a reader to see it — which is the same set of installs `preview::render`'s
//! `re_render` performs after a full render, with three differences and no others:
//!
//! 1. **Widgets are MERGED, not replaced.** The whole point of splicing is that a table
//!    below the toggled block is the same live widget afterwards. So every widget-bearing
//!    product is `survivors ++ the region's own` rather than PASS A's (whose widgets
//!    belong to a scratch buffer that has already been thrown away).
//! 2. **Only the region's controls are re-wired.** A surviving disclosure toggle still
//!    carries the handler it was given when it was built; connecting a second one would
//!    fold twice per click, which reads as a click that does nothing.
//! 3. **The reading position is held, and `re_render` deliberately holds none.** See
//!    [`ReaderAnchor`].

use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::codeview::CodePreviewView;
use crate::fold::{FoldKey, FoldState};
use crate::preview::build::attach_anchored;
use crate::preview::cells::{
    attach_cell_marker_widgets, collect_cell_labels, collect_table_anchors,
};
use crate::preview::qdata::{scrib_anchor_widgets, scrib_labels, scrib_render_data};

/// Everything a splice needs about the document, so the window layer hands it forward
/// once instead of each half reaching for `TabState` on its own.
pub(crate) struct SpliceInputs<'a> {
    pub(crate) md: &'a str,
    pub(crate) doc_dir: Option<&'a std::path::Path>,
    pub(crate) zoom: f64,
    pub(crate) allow_unsafe_images: bool,
    pub(crate) folds: &'a FoldState,
}

/// What an attempted in-place disclosure splice did to the pane.
///
/// Deliberately not a `bool`: two of the three answers mean "the splice did not
/// happen" and they carry DIFFERENT obligations, which is exactly the distinction a
/// `bool` erased when this returned one. See [`super::SpliceRefusal`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpliceVerdict {
    /// The region was written and adopted; the pane is current and nothing else is owed.
    Spliced,
    /// Nothing was attempted or the attempt was refused before the buffer was touched.
    /// The pane still holds the pre-toggle render, so a fallback re-render is what makes
    /// the toggle VISIBLE — not what makes the pane correct.
    Untouched,
    /// The buffer was mutated and the replacement region was never written. **The caller
    /// must re-render the pane whole**; there is no state in which leaving this is right.
    RegionLost,
}

impl SpliceVerdict {
    /// Whether the toggle landed by the splice route. `false` from either refusal — the
    /// caller falls back to a full re-render for both, and consults the variant only when
    /// it needs to know whether the pane it is falling back over is intact.
    pub(crate) fn spliced(self) -> bool {
        self == Self::Spliced
    }
}

impl From<super::SpliceRefusal> for SpliceVerdict {
    fn from(refusal: super::SpliceRefusal) -> Self {
        match refusal {
            super::SpliceRefusal::Untouched => Self::Untouched,
            super::SpliceRefusal::RegionLost => Self::RegionLost,
        }
    }
}

/// Toggle `key` in `view`'s live preview by splicing its region, holding the reader's
/// place across the change.
///
/// `inputs.folds` must already reflect the NEW state — the caller toggles it before
/// calling this, exactly as [`super::splice`] requires.
///
/// **Anything but [`SpliceVerdict::Spliced`] means the caller must consider a full
/// re-render**, and the verdict says whether that re-render is optional or mandatory.
/// [`SpliceVerdict::Untouched`] is a refusal made BEFORE the buffer was touched — the
/// view has no `RenderData` yet, or `key` names no disclosure this render drew, or the
/// recorded extent does not fit the live buffer — so the pane is still consistent.
/// [`SpliceVerdict::RegionLost`] is [`super::SpliceRefusal::RegionLost`] surfacing: the
/// delete ran, the region write did not, and the pane is now inconsistent until it is
/// re-rendered whole.
///
/// This used to be a `bool` documented as "every `false` is a refusal made BEFORE the
/// buffer is touched", which the region-write path falsified — and a caller reading
/// that sentence would have been entitled to leave a corrupted pane on screen.
pub(crate) fn splice_disclosure(
    view: &CodePreviewView,
    inputs: SpliceInputs<'_>,
    key: FoldKey,
) -> SpliceVerdict {
    let Some(render_data) = scrib_render_data(view) else {
        log::debug!(
            "preview::splice: refusing key {key:?} — the view holds no RenderData yet \
             (nothing has rendered into it); falling back to a full re-render"
        );
        return SpliceVerdict::Untouched;
    };
    let buf = view.buffer();

    // Read the PRE-splice render's own lists before anything moves. `anchored` is the
    // (anchor, widget) pairing the splice needs to compute survivors, and nothing but
    // the view holds it — `scrib_anchor_widgets` keeps widgets alone, for `re_render`'s
    // unparent sweep, so the anchors are recovered here from the widgets the view is
    // already parenting.
    let old_anchored = live_anchored(view, &buf);
    let old_extents = render_data.borrow().disclosure_extents.clone();

    // The reader's place, captured while the buffer still holds the old content — see
    // `ReaderAnchor`. Taken before the splice because there is nothing to recompute
    // from afterwards: the point of the exercise is a position that survives the edit.
    let anchor = ReaderAnchor::capture(view);

    // ONE preparation of the document, shared by the splice's two passes — see
    // `preview::build::Prepared`. It also retires the eleven-argument positional
    // hand-off this call used to be.
    let prepared = crate::preview::build::Prepared::new(
        inputs.md,
        inputs.doc_dir,
        inputs.zoom,
        inputs.allow_unsafe_images,
        crate::theme::active(),
        inputs.folds,
    );

    let outcome = match super::splice(
        &buf,
        Some(view),
        &old_anchored,
        &old_extents,
        &prepared,
        key,
    ) {
        Ok(outcome) => outcome,
        Err(refusal) => return SpliceVerdict::from(refusal),
    };

    install_outcome(view, &render_data, outcome, inputs.zoom);

    // AFTER the install, so the geometry the restore reads is the geometry of the
    // document the reader is now looking at.
    if let Some(anchor) = anchor {
        anchor.restore_when_settled(view);
    }
    SpliceVerdict::Spliced
}

/// The (anchor, widget) pairs currently live in `buf`, in document order.
///
/// The view's own anchored-children record is a `Vec<gtk::Widget>` (`re_render` needs
/// only that, to unparent them), and [`super::splice`] needs each widget's ANCHOR to
/// decide which ones the delete took with it. Recovered rather than stored a second
/// time: `TextChildAnchor::widgets()` is GTK's own pairing, so this cannot drift from
/// what the buffer actually holds, whereas a parallel list could.
fn live_anchored(
    view: &CodePreviewView,
    buf: &gtk::TextBuffer,
) -> Vec<(gtk::TextChildAnchor, gtk::Widget)> {
    let Some(widgets) = scrib_anchor_widgets(view) else {
        return Vec::new();
    };
    // A SET, not a `Vec`. This runs on the main loop for every fold toggle, and the
    // membership test used to be a linear scan inside a per-character walk — O(chars ×
    // widgets) on a document the reader is waiting on. glib objects hash and compare by
    // pointer, which is exactly the identity this test wants.
    let known: std::collections::HashSet<gtk::Widget> = widgets.borrow().iter().cloned().collect();
    let mut out: Vec<(gtk::TextChildAnchor, gtk::Widget)> = Vec::new();

    // Anchors sit at U+FFFC OBJECT REPLACEMENT CHARACTER, GTK's own placeholder for one,
    // so the scan skips between them inside GTK rather than paying a Rust closure per
    // character of the document. `forward_find_char` ADVANCES BEFORE it tests, so the
    // character at the start iterator is never offered to the predicate — hence the
    // separate first check rather than a bare loop, which would silently drop an anchor
    // sitting at offset 0.
    for anchor in anchors_in(buf) {
        for widget in anchor.widgets() {
            if known.contains(&widget) {
                out.push((anchor.clone(), widget));
            }
        }
    }
    out
}

/// U+FFFC OBJECT REPLACEMENT CHARACTER — the character `GtkTextBuffer` puts in the text
/// where a child anchor sits. Named rather than spelled inline because a bare `'\u{FFFC}'`
/// at a call site reads as a magic constant, and it is GTK's contract rather than ours.
const ANCHOR_PLACEHOLDER: char = '\u{FFFC}';

/// Every child anchor in `buf`, in document order.
///
/// Split out of [`live_anchored`] so its one sharp edge is reachable by a test that needs
/// no view: **`forward_find_char` advances BEFORE it tests**, so the character at the
/// start iterator is never offered to the predicate, and a bare loop silently drops an
/// anchor sitting at offset 0. That is not a hypothetical shape — a document whose first
/// block is a table or an image opens with one.
fn anchors_in(buf: &gtk::TextBuffer) -> Vec<gtk::TextChildAnchor> {
    let mut found = Vec::new();
    let mut iter = buf.start_iter();
    if let Some(anchor) = iter.child_anchor() {
        found.push(anchor);
    }
    while iter.forward_find_char(|c| c == ANCHOR_PLACEHOLDER, None) {
        if let Some(anchor) = iter.child_anchor() {
            found.push(anchor);
        }
    }
    found
}

/// Install a completed splice: the maps from PASS A, the widgets merged, the region's
/// controls wired.
///
/// The ORDER mirrors `re_render`'s, and the mirroring is the point — a spliced pane and
/// a re-rendered one must be indistinguishable to everything downstream, so the two
/// routes install the same set in the same sequence and differ only in where each
/// product came from.
fn install_outcome(
    view: &CodePreviewView,
    render_data: &Rc<RefCell<crate::preview::qdata::RenderData>>,
    outcome: super::SpliceOutcome,
    zoom: f64,
) {
    let super::SpliceOutcome {
        products,
        merged_anchored,
        region,
    } = outcome;
    let super::ScratchProducts {
        maps,
        decor,
        mut markers,
        cell_src_spans,
    } = products;

    // The widget half, which PASS A's output does not contain at all (see
    // `ScratchProducts`): the survivors of the delete, plus the region render's fresh
    // children. Built here because this is the only layer that can see both.
    let install = crate::preview::build::ViewInstall {
        decor,
        widgets: crate::preview::build::InstallWidgets {
            width_bounded: merge(view.width_bounded(), region.width_bounded, |(w, _)| {
                w.clone()
            }),
            image_bounded: merge(view.image_bounded(), region.image_bounded, |(w, _, _)| {
                w.clone()
            }),
            tables: merge(view.tables(), region.tables, |t| {
                t.clone().upcast::<gtk::Widget>()
            }),
        },
    };

    let buf = view.buffer();
    {
        let mut rd = render_data.borrow_mut();
        // PASS A's maps, wholesale — the same call `re_render` makes, which is what
        // keeps a spliced pane indistinguishable from a re-rendered one no matter how
        // many maps a render comes to produce. `disclosure_extents` is among them and
        // is read by the two steps below, so the write happens before this borrow is
        // dropped rather than in a second one.
        rd.adopt_maps(maps);
        // The widget-keyed halves, which `adopt_maps` deliberately does not own. This
        // route MERGES rather than replaces: the survivors are live children of a
        // buffer that was never swapped, and only the region's are new.
        rd.image_tints = merge_pairs(std::mem::take(&mut rd.image_tints), region.image_tints);
        rd.table_anchors = collect_table_anchors(&merged_anchored);
    }

    // The region's own controls: parented as they were created (`Renderer::set_live_view`
    // — `splice` is handed the view), so only the wiring and the line index remain.
    crate::preview::render::wire_spliced_disclosure_toggles(
        view,
        render_data,
        region.disclosure_toggles,
        &merged_anchored,
    );
    // Every surviving control is re-pointed at the state it now shows. The toggle the
    // reader clicked is a SURVIVOR — it sits on the summary line, above the region a
    // splice changes — so nothing else would move its arrow (rubric 2.26a). Driven from
    // PASS A's own `collapsed_blocks` rather than from the widget's `active`, so the
    // indicator follows the render and cannot disagree with it.
    refresh_disclosure_indicators(&buf, render_data, &merged_anchored, zoom);

    if let Some(tl) = scrib_labels(view) {
        *tl.borrow_mut() = collect_cell_labels(&merged_anchored);
    }
    if let Some(aw) = scrib_anchor_widgets(view) {
        *aw.borrow_mut() = merged_anchored.iter().map(|(_, w)| w.clone()).collect();
    }

    crate::preview::build::install_content(view, install, zoom);
    // ONLY the region's children are handed to `attach_anchored`, never the merged
    // list: it parents unconditionally, and a survivor is already parented (the module
    // docs name this as the caller's obligation). Here the region's are already
    // parented too — `splice` was given the view — so this is a no-op that exists to
    // keep the route honest if that ever changes, and to take the batched
    // `queue_resize` with it.
    attach_anchored(view, &[]);
    attach_cell_marker_widgets(&mut markers, &merged_anchored, &cell_src_spans);
    view.set_markers(markers);
}

/// Whatever of `previous` is still parented, plus everything in `fresh`.
///
/// **Survivorship is read from the widget, not inferred from the region.** GTK's own
/// delete unparents every child inside the deleted range and touches nothing outside
/// it, so "still has a parent" is exactly "the splice did not take this one" — the
/// same fact [`super::splice`]'s anchor merge reads through `is_deleted()`, asked of
/// the other end of the pair.
fn merge<T: Clone>(
    previous: Vec<T>,
    fresh: Vec<T>,
    widget_of: impl Fn(&T) -> gtk::Widget,
) -> Vec<T> {
    keep_survivors(previous, fresh, |item| widget_of(item).parent().is_some())
}

/// [`merge`] for the `(anchor, widget)` lists, where the anchor answers the same
/// question more directly than the widget's parent does.
fn merge_pairs(
    previous: Vec<(gtk::TextChildAnchor, gtk::Widget)>,
    fresh: Vec<(gtk::TextChildAnchor, gtk::Widget)>,
) -> Vec<(gtk::TextChildAnchor, gtk::Widget)> {
    keep_survivors(previous, fresh, |(anchor, _)| !anchor.is_deleted())
}

/// The merge RULE, with the GTK question factored out: whatever of `previous` survived,
/// in its original order, then everything in `fresh`.
///
/// Pure, so the rule is pinned without a display — and there are three separate ways to
/// get it wrong that all compile and all produce a plausible-looking list: dropping the
/// survivors, dropping the fresh entries, or reordering either group. The last is the
/// quiet one, because `collect_table_anchors` and `attach_cell_marker_widgets` both pair
/// this list POSITIONALLY against PASS A's `cell_src_spans`, so a transposition maps a
/// marker onto the wrong cell rather than failing.
fn keep_survivors<T>(previous: Vec<T>, fresh: Vec<T>, survived: impl Fn(&T) -> bool) -> Vec<T> {
    let mut out: Vec<T> = previous.into_iter().filter(|item| survived(item)).collect();
    out.extend(fresh);
    out
}

/// The reader's place, as a RELATIONSHIP rather than a position: how far their line sits
/// below the top of the viewport.
///
/// Negative or zero in practice — the line occupying the top of the viewport starts at
/// or above it — but nothing here depends on the sign.
fn offset_below_viewport_top(line_y: i32, adjustment_value: f64) -> f64 {
    f64::from(line_y) - adjustment_value
}

/// The adjustment value that puts a line now at `line_y` back at `offset` below the top.
///
/// The inverse of [`offset_below_viewport_top`], and the whole restore. Split out and
/// unit-tested because the plausible wrong spellings are all one token away and none of
/// them fails to compile: restoring the RECORDED VALUE (which ignores that the document
/// grew above the reader), restoring `line_y` alone (which puts their line flush with
/// the viewport's top edge rather than where it was), or adding the offset instead of
/// subtracting it (correct exactly when the offset is zero, which is most of the time on
/// a line that happens to start at the top).
fn restored_value(line_y: i32, offset: f64) -> f64 {
    f64::from(line_y) - offset
}

/// Re-point every live disclosure control at the state this render drew it in.
///
/// A full re-render never needs this: it rebuilds every control from
/// `widgets::disclosure::build(expanded, …)`. A splice keeps the ones outside its
/// region, and the one the reader clicked is always among them.
fn refresh_disclosure_indicators(
    buf: &gtk::TextBuffer,
    render_data: &Rc<RefCell<crate::preview::qdata::RenderData>>,
    merged_anchored: &[(gtk::TextChildAnchor, gtk::Widget)],
    zoom: f64,
) {
    let rd = render_data.borrow();
    for extent in &rd.disclosure_extents {
        let collapsed = rd.collapsed_blocks.iter().any(|c| c.key == extent.key);
        let Some(toggle) = toggle_at(buf, merged_anchored, extent.summary.start) else {
            continue;
        };
        crate::widgets::disclosure::set_expanded(&toggle, !collapsed, zoom);
    }
}

/// The disclosure control anchored at buffer char `offset`, if any.
///
/// The control's anchor sits at the START of its summary line, which is exactly
/// `DisclosureExtent::summary.start` — one fact recorded once by the renderer, read
/// from two of its outputs, rather than a widget-tree search for "something that looks
/// like a toggle".
fn toggle_at(
    buf: &gtk::TextBuffer,
    merged_anchored: &[(gtk::TextChildAnchor, gtk::Widget)],
    offset: i32,
) -> Option<gtk::ToggleButton> {
    merged_anchored.iter().find_map(|(anchor, widget)| {
        (buf.iter_at_child_anchor(anchor).offset() == offset)
            .then(|| widget.clone().downcast::<gtk::ToggleButton>().ok())
            .flatten()
    })
}

/// Where the reader was, expressed so that it survives an edit above them.
///
/// A `GtkTextMark` on the line at the top of the viewport, plus that line's pixel
/// distance below the viewport's top edge. Restoring means recomputing the line's `y`
/// in the NEW geometry and writing `y − offset` back: the reader's own line lands back
/// under their eye, whatever the edit did to the document's height above it.
///
/// # Why a restore and not a compensation
///
/// GTK 4.6–4.18 compensates an edit above the viewport by writing back
/// `first_para_top`'s delta on every layout `::changed`, and each of those passes lands
/// `top_margin` pixels short (fixed upstream in 4.19.3, commit `b300698629`). The
/// tempting fix is to add the missing quantum back — `emissions × top_margin` — and it
/// is wrong: the emission count is MEASURED BIMODAL at a fixed dose (55 or 63 for the
/// identical toggle), so a fixed correction is wrong some of the time whatever value it
/// takes, and it would become a DOUBLE correction the day the floor moves past 4.19.3.
/// A restore counts nothing. It reads the settled geometry and asks for the
/// relationship it recorded, so the bimodality cannot reach it — and on a GTK that
/// compensates correctly there is nothing left to move, so it degrades to a
/// same-value `set_value`, which `GtkAdjustment` swallows.
///
/// # The reader inside a collapsing block
///
/// TDD 2.26h's last clause — a reader inside the block they are collapsing settles on
/// its summary line — needs no branch here. GTK moves a mark inside a deleted range to
/// the deletion point, and the deletion point is `DisclosureExtent::volatile.start`,
/// which is the end of the summary label. So the mark lands ON the summary line and the
/// restore puts that line where the reader's old line was.
struct ReaderAnchor {
    mark: gtk::TextMark,
    /// The marked line's `y` minus the adjustment's `value`, in pixels — negative or
    /// zero, since the line at the top of the viewport starts at or above it.
    offset: f64,
}

impl Drop for ReaderAnchor {
    /// Delete the mark if nothing consumed the anchor.
    ///
    /// [`ReaderAnchor::capture`] runs BEFORE the splice, because the position it records
    /// only exists in the pre-splice buffer — so every refusal downstream of it (and
    /// there are several) returns while holding a mark nobody will ever delete. A
    /// `GtkTextBuffer` outlives every render, so those accumulate one per refused
    /// toggle, forever.
    ///
    /// A `Drop` rather than a delete at each early return: the refusals are in a
    /// different function from the capture, and a rule that must be re-obeyed at every
    /// new `return` is the rule that gets missed by the next one added. The consuming
    /// path suppresses this — see [`ReaderAnchor::restore_when_settled`].
    fn drop(&mut self) {
        if self.mark.is_deleted() {
            return;
        }
        if let Some(buffer) = self.mark.buffer() {
            buffer.delete_mark(&self.mark);
        }
    }
}

/// Why [`ReaderAnchor::restore_when_settled`] did not put the reader back.
///
/// A closed set with a name per arm rather than a bare `None`, because the arms are
/// different defects: the first two are geometry that never arrived, the third is the
/// splice having deleted the position it promised to restore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NotRestored {
    /// The view had no vertical adjustment — it is not inside a scroller.
    NoAdjustment,
    /// The adjustment reports no page size, so the viewport has never been allocated
    /// and there is no coordinate space to restore into.
    ViewportUnallocated,
    /// The mark the anchor was captured on is gone from the buffer.
    MarkDeleted,
    /// A later toggle claimed the restore while this one was waiting, so this one's
    /// anchor points into a buffer state the reader has already moved past.
    Superseded,
}

impl NotRestored {
    fn as_str(self) -> &'static str {
        match self {
            Self::NoAdjustment => "the view has no vadjustment",
            Self::ViewportUnallocated => "the viewport has no page size yet",
            Self::MarkDeleted => "the anchor mark was deleted",
            Self::Superseded => "a later toggle claimed the restore",
        }
    }
}

impl ReaderAnchor {
    /// `None` when there is nothing to hold: no viewport yet, so no reading position
    /// exists and `line_yrange` would answer about a layout that has never run.
    fn capture(view: &CodePreviewView) -> Option<Self> {
        let adjustment = view.vadjustment()?;
        if adjustment.page_size() <= 0.0 {
            return None;
        }
        // Through the seam, never a hand-rolled `line_at_y`: on a view with no
        // allocation that call answers the buffer's LAST line rather than declining
        // (ScrAP-263), and the answer is not distinguishable from a real one.
        let iter = crate::saferizer::viewport::ViewportTopIter::of(view);
        let (y, _height) = view.line_yrange(&iter);
        // Left gravity. The splice inserts strictly ABOVE this position, so gravity
        // cannot separate the two today — but a left-gravity mark also stays put if a
        // future caller ever splices at the reader's own line, which is the direction
        // that would silently move it.
        let mark = view.buffer().create_mark(None, &iter, true);
        Some(ReaderAnchor {
            mark,
            offset: offset_below_viewport_top(y, adjustment.value()),
        })
    }

    /// Put the reader back, once GTK has finished moving the viewport on its own.
    ///
    /// The wait is [`crate::farscroll::after_scroll_settles`], and it is not
    /// negotiable: `gtk_text_view_value_changed` destroys `first_validate_idle`
    /// (gtktextview.c:8437-8443), so every one of GTK's compensating writes orphans a
    /// scroll issued while the settle is running — a restore landed early is silently
    /// eaten rather than overridden, which looks exactly like a restore that was never
    /// written.
    fn restore_when_settled(self, view: &CodePreviewView) {
        // The deferred restore OWNS the mark from here: it has to survive until the
        // scroll settles, which is well after this call returns. So the refusal-path
        // `Drop` must not run — it would delete the mark out from under the closure and
        // the restore would report `MarkDeleted` every time. The closure below deletes
        // it itself, on every path, which is where the responsibility now sits.
        let anchor = std::mem::ManuallyDrop::new(self);
        let (mark, offset) = (anchor.mark.clone(), anchor.offset);
        // Claim the restore. A reader toggling two blocks in quick succession arms two of
        // these over the SAME adjustment — each with its own quiet-counter and each ending
        // in a `set_value` — and the older one's anchor was captured against a buffer that
        // no longer exists. Whichever fired last used to win, which is a race rather than
        // a rule.
        let generation = view.claim_restore();
        crate::farscroll::after_scroll_settles(view.upcast_ref(), move |view| {
            let buffer = view.buffer();
            let restored = (|| {
                // Superseded: a later toggle claimed the restore while this one was
                // waiting. Stand down WITHOUT scrolling — the newer restore owns the
                // reader's place, and writing this one's would move them off it. The mark
                // is still deleted below, on every path.
                if view
                    .downcast_ref::<CodePreviewView>()
                    .is_some_and(|v| v.restore_generation() != generation)
                {
                    return Err(NotRestored::Superseded);
                }
                let adjustment = view.vadjustment().ok_or(NotRestored::NoAdjustment)?;
                if adjustment.page_size() <= 0.0 {
                    return Err(NotRestored::ViewportUnallocated);
                }
                if mark.is_deleted() {
                    return Err(NotRestored::MarkDeleted);
                }
                let iter = buffer.iter_at_mark(&mark);
                let (y, _height) = view.line_yrange(&iter);
                // `jump`, never `scroll_to_mark`: this is "put the position here", and
                // it must supersede rather than queue — a queued scroll is what GTK's
                // own writes destroy. A same-value write is swallowed by
                // `GtkAdjustment`, which is what makes this a no-op on a GTK that
                // compensated correctly.
                crate::saferizer::scrollpos::jump(&adjustment, restored_value(y, offset));
                Ok(())
            })();
            // 2.26h's whole promise is that the reader keeps their place, and every way
            // this can fail leaves them somewhere they did not ask to be with nothing on
            // screen to say so. The reason is the useful half — "not restored" alone
            // cannot distinguish a pane that never got a viewport from a mark the splice
            // deleted, and those are different defects.
            if let Err(reason) = restored {
                log::debug!(
                    "preview::splice: reading position not restored ({})",
                    reason.as_str()
                );
            }
            // The mark has done its job either way; leaving it would accumulate one
            // per toggle in a buffer that outlives every render.
            if !mark.is_deleted() {
                buffer.delete_mark(&mark);
            }
        });
    }
}

#[cfg(test)]
mod decision_tests {
    use super::*;

    // ---- keep_survivors ------------------------------------------------------------
    //
    // The merge RULE, without a display. Three ways to get it wrong that all compile,
    // and the reordering one is the quiet one: the merged list is paired POSITIONALLY
    // against PASS A's `cell_src_spans`, so a transposition maps a marker onto the wrong
    // cell rather than failing.

    #[test]
    fn survivors_keep_their_order_and_the_fresh_entries_follow() {
        assert_eq!(
            keep_survivors(vec![1, 2, 3, 4], vec![7, 8], |n| *n != 2),
            vec![1, 3, 4, 7, 8]
        );
    }

    #[test]
    fn nothing_surviving_leaves_exactly_the_fresh_list() {
        // The whole-region case: everything the pane held was inside the splice.
        assert_eq!(
            keep_survivors(vec![1, 2], vec![7, 8], |_| false),
            vec![7, 8]
        );
    }

    #[test]
    fn a_region_that_drew_nothing_leaves_exactly_the_survivors() {
        // Collapsing a block draws no widget at all, which is not the same as an empty
        // pane — a merge that dropped the survivors here would silently unbind every
        // table below the block.
        assert_eq!(keep_survivors(vec![1, 2], vec![], |_| true), vec![1, 2]);
    }

    #[test]
    fn the_fresh_entries_are_never_filtered() {
        // The predicate answers about the PREVIOUS render only. A region render's own
        // children may legitimately not be parented yet (the deferred route), so
        // applying survivorship to them would discard the whole region.
        assert_eq!(keep_survivors(vec![], vec![7, 8], |_| false), vec![7, 8]);
    }

    // ---- the reading position, as arithmetic ---------------------------------------

    #[test]
    fn a_restore_reproduces_the_offset_it_recorded() {
        // The round trip, which is the whole contract: whatever the document did to the
        // line's y, the line comes back the same distance below the viewport's top.
        let offset = offset_below_viewport_top(11_700, 11_705.0);
        assert_eq!(offset, -5.0);
        // The block above the reader opened and pushed their line down by 1674px.
        assert_eq!(restored_value(11_700 + 1674, offset), 11_705.0 + 1674.0);
    }

    #[test]
    fn a_line_flush_with_the_viewport_top_restores_to_its_own_y() {
        // The degenerate case, and the reason `restored_value` needs a test of its own:
        // at offset 0 the correct spelling and the sign-flipped one agree, so a test
        // written only against this case passes on `y + offset`.
        let offset = offset_below_viewport_top(9_000, 9_000.0);
        assert_eq!(offset, 0.0);
        assert_eq!(restored_value(9_400, offset), 9_400.0);
    }

    #[test]
    fn the_sign_of_the_offset_is_load_bearing() {
        // `y + offset` would answer 9395 here, five pixels the wrong way, and would keep
        // answering it every toggle. Pinned because the two spellings differ by one
        // character and the wrong one is right whenever the offset is zero.
        assert_eq!(restored_value(9_400, -5.0), 9_405.0);
    }

    #[test]
    fn nothing_moving_above_the_reader_restores_the_value_it_started_from() {
        // A block BELOW the reader changes no y above them, so the restore computes the
        // value already standing — which `GtkAdjustment` swallows. That is what makes
        // this a no-op rather than a nudge, on a fixed GTK and on an untouched viewport
        // alike.
        let offset = offset_below_viewport_top(4_020, 4_048.5);
        assert_eq!(restored_value(4_020, offset), 4_048.5);
    }

    #[test]
    fn a_lost_region_never_reports_a_landed_splice() {
        // The defect this pins was a `bool`: `render_region` substituted an empty
        // renderer after the delete had already run, and the splice reported success —
        // so the caller kept a pane missing a whole block, with every map below the
        // splice off by its length. There is no reaching input for the substitution
        // itself, which is exactly why the MAPPING is pinned here: the one line that
        // could silently re-open it is this conversion answering `true`.
        assert!(!SpliceVerdict::from(super::super::SpliceRefusal::RegionLost).spliced());
        assert!(!SpliceVerdict::from(super::super::SpliceRefusal::Untouched).spliced());
        assert!(SpliceVerdict::Spliced.spliced());
    }

    #[test]
    fn the_two_refusals_stay_distinguishable_at_the_boundary() {
        // Both answer `spliced() == false`, and a caller that needs to know whether the
        // pane it is falling back over is INTACT reads the variant. Collapsing them
        // would compile and would lose that.
        assert_ne!(
            SpliceVerdict::from(super::super::SpliceRefusal::RegionLost),
            SpliceVerdict::from(super::super::SpliceRefusal::Untouched)
        );
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    /// A `ReaderAnchor` that nothing consumes deletes its own mark.
    ///
    /// `capture` runs before the splice and several refusals return after it, so without
    /// this the buffer accumulates one `GtkTextMark` per refused toggle for the lifetime
    /// of the tab. Nothing else in the tree would notice: a stray mark changes no text,
    /// fails no assertion, and is invisible to every other test.
    ///
    /// Mutation-checked: deleting the `Drop` impl fails the first assertion.
    #[gtktest::test]
    fn an_unconsumed_reader_anchor_deletes_its_own_mark() {
        let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
        buffer.set_text("one\ntwo\nthree\n");
        let mark = buffer.create_mark(None, &buffer.start_iter(), true);

        let watched = mark.clone();
        {
            let _anchor = ReaderAnchor { mark, offset: -3.0 };
            assert!(
                !watched.is_deleted(),
                "precondition: the mark is live while the anchor holds it"
            );
        }
        assert!(
            watched.is_deleted(),
            "dropping an unconsumed anchor deletes its mark from the buffer"
        );
    }

    /// ...and dropping one whose mark is ALREADY gone is a clean no-op rather than a
    /// second delete. Reachable for real: the splice can delete the region the mark sits
    /// in, and GTK deletes the marks inside a deleted range with it.
    #[gtktest::test]
    fn dropping_an_anchor_whose_mark_was_already_deleted_is_a_no_op() {
        let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
        buffer.set_text("one\ntwo\n");
        let mark = buffer.create_mark(None, &buffer.start_iter(), true);
        buffer.delete_mark(&mark);
        assert!(mark.is_deleted(), "precondition");
        drop(ReaderAnchor { mark, offset: 0.0 });
    }

    /// The anchor scan finds one at offset 0, one in the middle and one at the end.
    ///
    /// Mutation-checked: dropping the pre-loop `start_iter` check loses the first;
    /// replacing the placeholder scan with a bare `forward_char` walk keeps all three
    /// and is only slower, which is why the perf claim is NOT what this test pins — the
    /// EDGE is.
    #[gtktest::test]
    fn the_anchor_scan_includes_one_at_the_very_start() {
        let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
        buffer.set_text("mid\ntail");
        let first = buffer.create_child_anchor(&mut buffer.start_iter());
        let mut at_mid = buffer.iter_at_offset(3);
        let middle = buffer.create_child_anchor(&mut at_mid);
        let last = buffer.create_child_anchor(&mut buffer.end_iter());

        let found = anchors_in(&buffer);
        assert_eq!(
            found.len(),
            3,
            "every anchor is found, the offset-0 one included"
        );
        assert_eq!(found[0], first, "the anchor at offset 0 is first");
        assert_eq!(found[1], middle);
        assert_eq!(found[2], last);
    }

    /// A buffer with no anchors yields none, and the scan terminates.
    #[gtktest::test]
    fn the_anchor_scan_terminates_on_a_buffer_with_none() {
        let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
        buffer.set_text("just text\nand more\n");
        assert!(anchors_in(&buffer).is_empty());
    }

    /// Claiming a restore supersedes the one before it, and the generation is what a
    /// pending restore compares itself against.
    ///
    /// The race this settles needs two settle-waits pending over one adjustment, which is
    /// not constructible without a presented window and real geometry — so what is pinned
    /// here is the DECISION, at the seam the deferred closure reads. Mutation-checked:
    /// making `claim_restore` return the generation without storing it leaves the first
    /// assertion's two values equal.
    #[gtktest::test]
    fn claiming_a_restore_supersedes_the_previous_claim() {
        let view = CodePreviewView::new();
        let first = view.claim_restore();
        let second = view.claim_restore();
        assert_ne!(first, second, "each claim gets its own generation");
        assert_eq!(
            view.restore_generation(),
            second,
            "the live generation is the LATEST claim, so the earlier one now differs"
        );
        assert_ne!(
            view.restore_generation(),
            first,
            "which is exactly the test a superseded restore makes on itself"
        );
    }
}
