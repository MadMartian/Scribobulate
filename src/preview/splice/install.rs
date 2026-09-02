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
use crate::preview::sourcemap::invert_source_map;

/// Everything a splice needs about the document, so the window layer hands it forward
/// once instead of each half reaching for `TabState` on its own.
pub(crate) struct SpliceInputs<'a> {
    pub(crate) md: &'a str,
    pub(crate) doc_dir: Option<&'a std::path::Path>,
    pub(crate) zoom: f64,
    pub(crate) allow_unsafe_images: bool,
    pub(crate) folds: &'a FoldState,
}

/// Toggle `key` in `view`'s live preview by splicing its region, holding the reader's
/// place across the change.
///
/// `inputs.folds` must already reflect the NEW state — the caller toggles it before
/// calling this, exactly as [`super::splice`] requires.
///
/// **Returns `false` when the splice could not be attempted or could not complete**,
/// and the caller must then fall back to a full re-render. Every `false` here is a
/// refusal made BEFORE the buffer is touched, so a fallback re-render is always
/// operating on an untouched pane. The refusals are: the view has no `RenderData` yet
/// (nothing has rendered), or `key` names no disclosure this render drew (an ancestor
/// was collapsed, or the fold state and the render have diverged) — [`super::splice`]'s
/// own two `None` cases.
pub(crate) fn splice_disclosure(
    view: &CodePreviewView,
    inputs: SpliceInputs<'_>,
    key: FoldKey,
) -> bool {
    let Some(render_data) = scrib_render_data(view) else {
        return false;
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

    let Some(outcome) = super::splice(
        &buf,
        Some(view),
        &old_anchored,
        &old_extents,
        inputs.md,
        inputs.doc_dir,
        inputs.zoom,
        inputs.allow_unsafe_images,
        crate::theme::active(),
        inputs.folds,
        key,
    ) else {
        return false;
    };

    install_outcome(view, &render_data, outcome, inputs.zoom);

    // AFTER the install, so the geometry the restore reads is the geometry of the
    // document the reader is now looking at.
    if let Some(anchor) = anchor {
        anchor.restore_when_settled(view);
    }
    true
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
    let known: Vec<gtk::Widget> = widgets.borrow().clone();
    let mut out: Vec<(gtk::TextChildAnchor, gtk::Widget)> = Vec::new();
    let mut iter = buf.start_iter();
    loop {
        if let Some(anchor) = iter.child_anchor() {
            for widget in anchor.widgets() {
                if known.contains(&widget) {
                    out.push((anchor.clone(), widget));
                }
            }
        }
        if !iter.forward_char() {
            break;
        }
    }
    out
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
    let crate::preview::build::RenderProducts {
        buf: _,
        disclosure_toggles: _, // PASS A's belong to the scratch buffer; `region`'s are live
        collapsed_blocks,
        disclosure_extents,
        source_map,
        copymap,
        md_owned,
        links,
        anchored: _, // ditto
        image_tints: _,
        mut install,
        heading_sites,
        heading_map,
        mut markers,
        cell_src_spans,
        highlight_ranges: _,
        shifts,
        original_owned,
    } = products;

    // The three widget-bearing halves of `ViewInstall`, merged. PASS A built these
    // against a scratch buffer whose widgets were never parented anywhere, so they are
    // the ONE part of its output that cannot be installed wholesale.
    install.width_bounded = merge(view.width_bounded(), region.width_bounded, |(w, _)| {
        w.clone()
    });
    install.image_bounded = merge(view.image_bounded(), region.image_bounded, |(w, _, _)| {
        w.clone()
    });
    install.tables = merge(view.tables(), region.tables, |t| {
        t.clone().upcast::<gtk::Widget>()
    });

    let buf = view.buffer();
    {
        let mut rd = render_data.borrow_mut();
        rd.source_map_inv = invert_source_map(&source_map);
        rd.source_map = source_map;
        rd.copymap = copymap;
        rd.md_owned = md_owned;
        rd.links = links;
        rd.heading_map = heading_map;
        rd.heading_sites = heading_sites;
        rd.collapsed_blocks = collapsed_blocks;
        rd.image_tints = merge_pairs(std::mem::take(&mut rd.image_tints), region.image_tints);
        rd.table_anchors = collect_table_anchors(&merged_anchored);
        rd.shifts = shifts;
        rd.original_owned = original_owned;
        // Last of the maps, and read by the two steps below, so it is written before
        // the borrow is dropped rather than in a second one.
        rd.disclosure_extents = disclosure_extents;
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
        let ReaderAnchor { mark, offset } = self;
        crate::farscroll::after_scroll_settles(view.upcast_ref(), move |view| {
            let buffer = view.buffer();
            let restored = (|| {
                let adjustment = view.vadjustment()?;
                if adjustment.page_size() <= 0.0 {
                    return None;
                }
                if mark.is_deleted() {
                    return None;
                }
                let iter = buffer.iter_at_mark(&mark);
                let (y, _height) = view.line_yrange(&iter);
                // `jump`, never `scroll_to_mark`: this is "put the position here", and
                // it must supersede rather than queue — a queued scroll is what GTK's
                // own writes destroy. A same-value write is swallowed by
                // `GtkAdjustment`, which is what makes this a no-op on a GTK that
                // compensated correctly.
                crate::saferizer::scrollpos::jump(&adjustment, restored_value(y, offset));
                Some(())
            })();
            let _ = restored;
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
}
