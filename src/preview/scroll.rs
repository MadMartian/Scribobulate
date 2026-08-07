//! Scroll-position capture and validation-safe restore for the preview/editor
//! scrollers. All restores go through a `GtkTextMark` + `scroll_to_mark` (never a
//! one-shot adjustment `set_value`), the pattern that survives GtkTextView's lazy
//! line-height validation without wedging input (ScrAP-14/22/65).

use super::qdata::scrib_render_data;
use crate::codeview::CodePreviewView;
use gtk::prelude::*;
use gtk::{ScrolledWindow, TextView};
use std::cell::RefCell;
use std::rc::Rc;

/// Fractional scroll position of a `GtkAdjustment`: 0.0 (top) … 1.0 (bottom).
/// A fraction is stable across re-renders even though the absolute pixel range
/// changes — the basis for both reload-position restore and split-pane sync.
pub(crate) fn adj_fraction(adj: &gtk::Adjustment) -> f64 {
    let max = adj.upper() - adj.page_size();
    if max > 0.0 {
        (adj.value() / max).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Set a `GtkAdjustment` to a fractional position, returning whether it could.
///
/// A freshly built scroller has no scroll range until it lays out — and a
/// `GtkTextView` measures its height lazily, so `upper` is a stale estimate at
/// first. During that window `upper - page_size` is ≤ 0, and applying the
/// fraction would compute `fraction * 0 = 0` and snap the view to the **top**
/// (the bug seen when switching edit → split). So when the range is not yet
/// established we leave the value untouched and report `false`, letting the
/// caller retry on the adjustment's next `changed` once a real range exists.
pub(crate) fn set_adj_fraction(adj: &gtk::Adjustment, fraction: f64) -> bool {
    let max = adj.upper() - adj.page_size();
    if max > 0.0 {
        adj.set_value(fraction * max);
        true
    } else {
        false
    }
}

/// Scroll a preview scroller (a `ScrolledWindow` wrapping the `CodePreviewView`) so
/// the heading at document-order index `doc_index` sits at the top of the view.
///
/// The buffer offset comes from the per-render `RenderData.heading_offsets` (same
/// document order as `outline::extract_headings`), so the outline's `doc_index`
/// maps straight to a scroll target without re-parsing.  A no-op if the scroller
/// does not wrap a preview view or the index is out of range (e.g. headings
/// changed).
///
/// Scrolling goes through a `GtkTextMark` + `scroll_to_mark`, NOT `scroll_to_iter`:
/// `scroll_to_iter` scrolls *immediately* using whatever line heights are computed
/// so far, but line-height validation runs in an idle pass.  Called right after a
/// re-render or while the `GtkPaned` is still settling, it pushed the adjustment
/// into an as-yet-unvalidated region — the preview went blank-gray and GTK spammed
/// "snapshot … without a current allocation" every frame (GTK4Rs/AP-22).  `scroll_to_mark`
/// records the target and defers the scroll until after validation, which is robust.
pub(crate) fn scroll_preview_to_heading(sw: &ScrolledWindow, doc_index: usize) {
    let Some(view) = sw
        .child()
        .and_then(|c| c.downcast::<CodePreviewView>().ok())
    else {
        return;
    };
    let Some(rd) = scrib_render_data(&view) else {
        return;
    };
    let offset = rd.borrow().heading_offsets.get(doc_index).copied();
    if let Some(offset) = offset {
        // The view owns the coalesced, validation-safe scroll (mark + scroll_to_mark
        // on an idle, with rapid re-targeting collapsed to the latest) — GTK4Rs/AP-22.
        view.scroll_to_buffer_offset(offset);
    }
}

/// Scroll a preview scroller (a `ScrolledWindow` wrapping the `CodePreviewView`)
/// so the heading whose anchor slug matches `fragment` sits at the top of the
/// view, consulting **that view's own** `RenderData.heading_map` — the same map
/// a same-document `#anchor` click reads in `preview/interactions.rs`. Returns
/// whether a match was found; a `false` is not itself an error — the caller
/// decides what "no such heading" means (today, `window::linknav` treats an
/// unmatched cross-document fragment exactly like a same-document anchor that
/// slugs to nothing: silent, the document simply opens at the top).
///
/// Same mark-based `scroll_to_buffer_offset` [`scroll_preview_to_heading`] uses,
/// for the same reason (GTK4Rs/AP-22). This is called right after a fresh render (a
/// just-created tab) or a just-materialized deferred one — exactly the moment
/// line heights are least likely to be validated yet — so a one-shot
/// `scroll_to_iter` here would risk the blank-gray / "without a current
/// allocation" hazard; `scroll_to_mark` defers the actual scroll until
/// validation catches up.
pub(crate) fn scroll_preview_to_fragment(sw: &ScrolledWindow, fragment: &str) -> bool {
    let Some(view) = sw
        .child()
        .and_then(|c| c.downcast::<CodePreviewView>().ok())
    else {
        return false;
    };
    let Some(rd) = scrib_render_data(&view) else {
        return false;
    };
    let offset = rd.borrow().heading_map.get(fragment).copied();
    match offset {
        Some(offset) => {
            view.scroll_to_buffer_offset(offset);
            true
        }
        None => false,
    }
}

/// Fractional scroll position of a preview/editor scroller.
pub(crate) fn preview_scroll_fraction(sw: &ScrolledWindow) -> f64 {
    adj_fraction(&sw.vadjustment())
}

/// The top visible buffer line of a preview/editor scroller (a `ScrolledWindow`
/// wrapping a `GtkTextView`). A *line* anchor, not a pixel fraction: it is
/// invariant across a zoom re-render (identical content, only rescaled), so a
/// same-buffer restore lands the exact same line back at the top — unlike a
/// `value/(upper−page_size)` fraction, which mixes tall (heading) and short
/// (blank) line heights and therefore drifts upward on a zoom re-render
/// (ScrAP-65). Capture BEFORE mutating the buffer,
/// while the old layout is still valid. `None` when the scroller does not wrap a
/// `GtkTextView`.
pub(crate) fn preview_top_line(sw: &ScrolledWindow) -> Option<i32> {
    let child = sw.child()?;
    // The preview is a CodePreviewView, which tracks a rapid-zoom-robust reading
    // anchor (its live top line, or the target of a still-settling programmatic
    // scroll) — ScrAP-65.
    if let Ok(cpv) = child.clone().downcast::<CodePreviewView>() {
        return Some(cpv.reading_line());
    }
    let view = child.downcast::<TextView>().ok()?;
    let y_top = sw.vadjustment().value() as i32;
    let (top_iter, _) = view.line_at_y(y_top);
    Some(top_iter.line())
}

/// Restore a `GtkTextView` scroller to an exact buffer `line`, validation-safe
/// and input-wedge-proof (ScrAP-65 — researcher-sourced from
/// gtktextview.c 4.6.9). Deferred to an idle where it:
///   1. scrolls to a left-gravity mark at `line` (yalign 0 = top). The
///      vadjustment's `upper` is finalised by `scroll_to_mark`'s internal
///      `flush_scroll` → `validate_yrange` — the ONLY validation force on this
///      path. (A prior `line_yrange(end)` pre-read used to precede this as a
///      supposed primer; the researcher confirmed against GTK 4.6.9 that
///      `gtk_text_view_get_line_yrange` validates NOTHING on any path — pure
///      cached-height btree read — so it was vestigial and has been removed;
///      ANTI-PATTERNS deferred-work meta-pattern, myth-bust #1.) A stale (too
///      small) `upper` both biases the restore toward the top (the zoom
///      scroll-drift) and lets `scroll_to_mark`'s `animate_to_value` freeze the
///      range collapsed — `size_allocate` skips `set_vadjustment_values` while
///      the adjustment is animating (gtktextview.c:4660), so `upper − page_size`
///      stays ≈ 0 and the wheel + PageUp/PageDown all go dead (the
///      input-wedge);
///   2. applies a NON-animating `set_value` clamp — belt-and-braces that
///      re-enables the size-allocate adjustment refresh a running animation
///      would otherwise keep suppressed, so a collapsed range can never persist.
///
/// This is the exact pattern `CodePreviewView::scroll_to_buffer_offset` already
/// proved for outline navigation (GTK4Rs/AP-22).
///
/// **Deferred-idle discipline (ScrAP-152).** The idle must WEAK-capture the view
/// and the scroller, and re-check `is_realized()` after upgrading. A strong
/// capture pins the widget alive as an *unrooted zombie* past `window.destroy()`
/// — unrealize is synchronous, finalize is not — and this idle then drives
/// `scroll_to_mark` on a view whose surface is gone (SIGSEGV). Note that
/// `upgrade()` succeeding is NOT the guard: the zombie upgrades fine, precisely
/// because this closure is what is keeping it alive. `CodePreviewView` gets both
/// halves plus coalescing and a cancel-on-reschedule from its
/// `schedule_scroll_idle` choke point; a base `GtkTextView` (the source editor)
/// has no `imp` slot to cancel through, so the two halves are applied here.
fn restore_textview_scroll_to_line(sw: &ScrolledWindow, view: &TextView, line: i32) {
    let sw_weak = sw.downgrade();
    let view_weak = view.downgrade();
    gtk::glib::idle_add_local_once(move || {
        // Liveness (upgrade) and rootedness (is_realized) are DIFFERENT questions
        // and both must be asked — ScrAP-152's whole point.
        let (Some(view), Some(sw)) = (view_weak.upgrade(), sw_weak.upgrade()) else {
            return; // widget dropped before the idle ran
        };
        if !view.is_realized() {
            return; // torn down (or not yet on screen) — nothing to scroll
        }
        let buffer = view.buffer();
        let clamped = line.clamp(0, (buffer.line_count() - 1).max(0));
        let iter = buffer
            .iter_at_line(clamped)
            .unwrap_or_else(|| buffer.end_iter());
        // Reuse a single persistent mark (moved, never recreated) — mirrors
        // `CodePreviewView::scroll_to_buffer_offset` exactly; a create+delete each
        // call raced GTK's first-paragraph pinning and left the view at the top.
        // The shared idiom lives in `codeview::move_or_create_mark` (QA M-5).
        let mark = crate::codeview::move_or_create_mark(&buffer, "scrib-scroll-restore", &iter);
        // (1) authoritative, validation-safe scroll to the top of `line`;
        // scroll_to_mark's internal flush_scroll finalises `upper` (the only
        // validation force here — see the doc comment's myth-bust #1 note).
        view.scroll_to_mark(&mark, 0.0, true, 0.0, 0.0);
        // (2) non-animating clamp re-enables the size-allocate refresh path.
        let vadj = sw.vadjustment();
        let max = (vadj.upper() - vadj.page_size()).max(0.0);
        if vadj.value() > max {
            vadj.set_value(max);
        }
    });
}

/// Restore a scroller to an exact buffer `line` — the *same-buffer* entry point
/// (a zoom re-render or reload, where the rendered content is unchanged and only
/// its scale differs, so an exact line anchor preserves the reading position with
/// none of the pixel-fraction drift). No-op for `line <= 0` (already at/above the
/// top) or a non-`GtkTextView` scroller.
pub(crate) fn restore_preview_scroll_to_line(sw: &ScrolledWindow, line: i32) {
    if line <= 0 {
        return;
    }
    let Some(child) = sw.child() else { return };
    // The preview is a CodePreviewView — reuse its proven, coalesced,
    // validation-forcing scroll (GTK4Rs/AP-22), targeting the buffer offset of `line`.
    if let Ok(cpv) = child.clone().downcast::<CodePreviewView>() {
        let offset = cpv
            .buffer()
            .iter_at_line(line)
            .map(|i| i.offset())
            .unwrap_or(0);
        cpv.scroll_to_buffer_offset(offset);
        return;
    }
    // Any other GtkTextView scroller — generic validation-safe restore.
    if let Ok(view) = child.downcast::<TextView>() {
        restore_textview_scroll_to_line(sw, &view, line);
    }
}

/// Restore a scroller to an exact buffer `line` on a **freshly-rendered** view — the
/// external-reload path rebuilds the whole preview widget, so its line heights are
/// unvalidated. The one-shot [`restore_preview_scroll_to_line`] fails for a FAR
/// target there: on a brand-new `GtkTextView`, `scroll_to_mark` →
/// `get_iter_location` → `find_line_top` sums CACHED (=0) heights from the buffer
/// start, so a mark ~20k lines down lands near the TOP (`flush_scroll`'s local
/// ±2×height validate never reaches the lines above it, and `pending_scroll` is a
/// ONE-SHOT that first-para pinning then defends near the top;
/// researcher-sourced from gtktextview.c/gtktextbtree.c 4.6.9).
///
/// Fix (researcher "pattern A", public API only): keep a left-gravity mark at the
/// target line and PROGRESSIVELY re-apply a NON-animating `set_value(line_yrange.y)`
/// as GTK validates heights top→down. `line_yrange(mark).y` grows monotonically
/// toward the true offset as the heights above the target fill in; each growth fires
/// the vadjustment's `notify::upper`, re-applying until `line_at_y(value)` lands on
/// the target line (or the range clamps at the bottom). Non-animating avoids the
/// `size_allocate` refresh freeze a running scroll animation causes
/// (gtktextview.c:4656-4660), and this is the normal scroll path (not a
/// pre-allocation `scroll_to_iter`), so it never re-enters GTK4Rs/AP-22.
pub(crate) fn restore_preview_scroll_to_line_fresh(sw: &ScrolledWindow, line: i32) {
    if line <= 0 {
        return;
    }
    // The preview is a CodePreviewView, but every step here is generic GtkTextView
    // geometry (line_yrange / line_at_y / iter_at_mark), so treat it as its base.
    let Some(view) = sw.child().and_then(|c| c.downcast::<TextView>().ok()) else {
        return;
    };
    restore_textview_scroll_to_line_progressive(sw, &view, line);
}

/// The progressive, `notify::upper`-driven far-restore core (see
/// [`restore_preview_scroll_to_line_fresh`]).
fn restore_textview_scroll_to_line_progressive(sw: &ScrolledWindow, view: &TextView, line: i32) {
    let buffer = view.buffer();
    let target = line.clamp(0, (buffer.line_count() - 1).max(0));
    let iter = buffer
        .iter_at_line(target)
        .unwrap_or_else(|| buffer.end_iter());
    // One persistent left-gravity mark (moved, never recreated) — the shared
    // `codeview::move_or_create_mark` idiom (QA M-5). Same mark name as the
    // one-shot restore, deliberately: both restore the same reading position.
    let mark = crate::codeview::move_or_create_mark(&buffer, "scrib-scroll-restore", &iter);
    // Pair the persisted mark with its owning buffer: this closure fires across many
    // `notify::upper` passes and a `re_render` (`set_buffer`) in between can orphan the
    // mark, so resolution must be membership-gated (ScrAP-104) via the one safe path.
    let bmark = crate::saferizer::buffer_mark::BufferMark::new(mark, &buffer);

    // `apply` tracks the target line's top-y DOWN as heights validate top→down;
    // returns true once settled (landed on the target, or clamped at the bottom).
    // WEAK captures only: the closure is stored on the vadjustment, which the
    // ScrolledWindow — and, transitively, the view — owns; a strong capture would
    // cycle and strand the whole preview subtree.
    let view_weak = view.downgrade();
    let sw_weak = sw.downgrade();
    let apply = Rc::new(move || -> bool {
        let (Some(view), Some(sw)) = (view_weak.upgrade(), sw_weak.upgrade()) else {
            return true; // widget gone (a later reload replaced it) — stop
        };
        if !view.is_mapped() {
            return false; // not on screen yet — wait for the first allocation
        }
        let vadj = sw.vadjustment();
        let max = (vadj.upper() - vadj.page_size()).max(0.0);
        if max <= 0.0 {
            return false; // range not established yet (page_size 0 pre-allocate)
        }
        // Cross-buffer guard (ScrAP-104). The mark was created on the buffer
        // that was live when this restore was scheduled; this closure fires across
        // MANY `notify::upper` passes, and a `re_render` (`set_buffer`) in between
        // finalizes that old buffer, ORPHANING the mark. `iter_at_mark` has NO
        // mark∈buffer check in GTK 4.6 — an orphaned/foreign mark yields an
        // UNINITIALISED iter whose `.line()`/`line_yrange` then aborts the process with
        // the fatal `gtk_text_btree_line_number couldn't find line`. The seam returns
        // `None` unless the mark still belongs to the live buffer; bail then (return
        // `true` = settled → disconnects); a fresh render installs its own restore.
        let Some(iter) = bmark.resolve(&view.buffer()) else {
            return true;
        };
        let (y, _) = view.line_yrange(&iter);
        vadj.set_value((y as f64).clamp(0.0, max)); // NON-animating
        let (top, _) = view.line_at_y(vadj.value() as i32);
        top.line() == target || vadj.value() >= max
    });

    // Self-disconnecting notify::upper subscription: re-apply on each validation
    // growth until settled (captures `adj` from the param, never a strong external
    // ref — same non-cyclic pattern as `restore_preview_scroll`'s changed handler).
    let handler: Rc<RefCell<Option<gtk::glib::SignalHandlerId>>> = Rc::new(RefCell::new(None));
    let handler_notify = Rc::clone(&handler);
    let apply_notify = Rc::clone(&apply);
    let id = sw.vadjustment().connect_upper_notify(move |adj| {
        if apply_notify() {
            if let Some(hid) = handler_notify.borrow_mut().take() {
                adj.disconnect(hid);
            }
        }
    });
    *handler.borrow_mut() = Some(id);

    // Kick off after the first layout wave — also settles an already-warm view in
    // one pass (no notify::upper would follow there).
    let apply_idle = Rc::clone(&apply);
    let handler_idle = Rc::clone(&handler);
    let vadj = sw.vadjustment();
    gtk::glib::idle_add_local_once(move || {
        if apply_idle() {
            if let Some(hid) = handler_idle.borrow_mut().take() {
                vadj.disconnect(hid);
            }
        }
    });
}

/// Restore a captured scroll `fraction` onto a freshly rendered scroller (TDD
/// 3.2 / 7.4 — reading position survives an external reload or a mode switch).
///
/// When the scroller wraps a `GtkTextView` (the preview *and* the editor are both
/// TextViews) the restore goes through a mark + `scroll_to_mark`, never an
/// adjustment `set_value` (ScrAP-14). A fresh `GtkTextView` validates line heights
/// over many idle passes, so its adjustment `upper` is a *draft* at the first
/// `changed`; `set_value(fraction × (upper − page))` then lands on a pre-validation
/// height and is overwritten by the next pass — the view snaps back toward the top.
/// Wrapping the content in the outline `GtkPaned` made this worse, because the
/// paned defers its position to `connect_map`, re-allocating the content width
/// (hence re-validating heights) *after* a one-shot adjustment restore had already
/// fired and disconnected — the mode-switch scroll-reset regression.
/// `scroll_to_mark` records the target line and GTK re-applies it after every
/// validation pass and re-allocation until stable, so it is robust to both.
///
/// The target line is `fraction × line_count` (a line-fraction, not the old
/// pixel-fraction). Source and rendered views have different per-line heights, so
/// no offset is exact across a mode switch anyway; a line-fraction is stable and
/// close, and — crucially — survives validation, which the pixel-fraction did not.
pub(crate) fn restore_preview_scroll(sw: &ScrolledWindow, fraction: f64) {
    if fraction <= 0.0 {
        return;
    }
    // Preferred path: a GtkTextView (preview CodePreviewView or the editor View,
    // both TextView subclasses). This is the CROSS-buffer restore (a view-mode
    // switch: the rendered preview and the source editor have different line
    // counts and per-line heights), so the captured pixel fraction is mapped to a
    // LINE fraction — an accepted approximation across that boundary (a zoom
    // re-render uses `restore_preview_scroll_to_line` instead, which is exact).
    // The restore itself is validation-safe and input-wedge-proof
    // (ScrAP-65) via the shared core.
    //
    // The `CodePreviewView` arm is tried FIRST, exactly as
    // `restore_preview_scroll_to_line` does. It is not an optimisation: that
    // subclass routes through `schedule_scroll_idle`, which coalesces, cancels
    // the previous idle, weak-captures and gates on `is_realized` (ScrAP-152).
    // Downcasting straight to the base `TextView` — which this function used to
    // do — silently skipped all of that for the ONE view most likely to be torn
    // down mid-restore, since a view-mode switch destroys and rebuilds the
    // preview. The two callers must not disagree about which path the same
    // widget takes.
    if let Some(child) = sw.child() {
        if let Ok(cpv) = child.clone().downcast::<CodePreviewView>() {
            let lines = cpv.buffer().line_count();
            if lines > 0 {
                let target = ((fraction * lines as f64).round() as i32).clamp(0, lines - 1);
                let offset = cpv
                    .buffer()
                    .iter_at_line(target)
                    .map(|i| i.offset())
                    .unwrap_or(0);
                cpv.scroll_to_buffer_offset(offset);
            }
            return;
        }
        // Any other GtkTextView scroller (the source editor) — generic
        // validation-safe restore, itself weak-captured and realize-gated.
        if let Ok(view) = child.downcast::<TextView>() {
            let lines = view.buffer().line_count();
            if lines > 0 {
                let target = ((fraction * lines as f64).round() as i32).clamp(0, lines - 1);
                restore_textview_scroll_to_line(sw, &view, target);
            }
            return;
        }
    }
    // Fallback (non-TextView scroller): adjustment fraction, applied once a real
    // range exists. Applying exactly once (not on every `changed`) is deliberate —
    // a persistent restorer would keep yanking the view back to the captured
    // position whenever its content changed.
    let vadj = sw.vadjustment();
    if set_adj_fraction(&vadj, fraction) {
        return; // range already known — applied immediately
    }
    let handler: Rc<RefCell<Option<gtk::glib::SignalHandlerId>>> = Rc::new(RefCell::new(None));
    let handler_inner = Rc::clone(&handler);
    let id = vadj.connect_changed(move |adj| {
        if set_adj_fraction(adj, fraction) {
            if let Some(hid) = handler_inner.borrow_mut().take() {
                adj.disconnect(hid);
            }
        }
    });
    *handler.borrow_mut() = Some(id);
}

/// GTK-object tests: building a `GtkTextView`/`ScrolledWindow` and driving a
/// `GtkTextBuffer` need an initialized GTK, so — like `window/reload.rs` — these
/// use `#[gtktest::test]` behind the `gtk-integration-tests` feature.
///
/// ## Why the snap-to-top *regression* is verified in MANUAL-TEST 3.2a,
/// not here
///
/// The bug is that a fresh (rebuilt) preview widget has UNVALIDATED line heights
/// when the far-restore fires, so the old one-shot `scroll_to_mark` summed cached
/// (=0) heights and landed near the top. Reproducing that condition requires a
/// live, mapped view whose heights are still *draft* at restore time. Attempts to
/// stage it headlessly fail in both directions: pumping the loop until the window
/// is mapped+allocated (below) also VALIDATES the heights, so even the pre-fix
/// one-shot lands correctly (QA Round-4 mutation test confirmed this); not pumping
/// at all leaves `is_mapped()` false so the progressive path never runs either. So
/// the actual snap-to-top settling is a genuinely GUI-only path: POLICY's
/// "regression coverage — two independent areas" is satisfied by the automated
/// tests below (decidable: mark placement, no-panic on a huge buffer, no-op at the
/// top, and viewport-reaches-target on a *mapped* view) PLUS **MANUAL-TEST 3.2a as
/// the area-2 live regression guard** (POLICY §"Manual integration testing" —
/// live-verify GUI-only behaviour the unit path can't observe). These tests
/// therefore assert what they can honestly prove; none of them claims to be the
/// snap-to-top discriminator.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    /// Watchdog deadline for a `pump_until` loop.
    const PUMP_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);

    /// Pump the GTK main loop until `done()` returns true, or panic with `msg` after
    /// [`PUMP_DEADLINE`]. The watchdog is a real glib TIMEOUT SOURCE, not a
    /// between-iterations clock check: a blocking `iteration(true)` only returns when
    /// the context has work, so on a truly idle display an `Instant` comparison after
    /// the call would never be reached (QA L-1). The timeout source IS dispatchable
    /// work, so it guarantees `iteration(true)` returns by the deadline, at which
    /// point the loop sees the flag and asserts instead of hanging. The source is
    /// removed on the normal (converged) exit so it can't fire into a later test.
    fn pump_until(ctx: &gtk::glib::MainContext, msg: &str, mut done: impl FnMut() -> bool) {
        use std::cell::Cell;
        use std::rc::Rc;
        let fired = Rc::new(Cell::new(false));
        let f = fired.clone();
        let source = gtk::glib::timeout_add_local_once(PUMP_DEADLINE, move || f.set(true));
        let mut source = Some(source);
        loop {
            if done() {
                break;
            }
            assert!(
                !fired.get(),
                "pump watchdog ({PUMP_DEADLINE:?}) fired: {msg}"
            );
            ctx.iteration(true);
        }
        // Converged before the deadline → cancel the still-pending watchdog source.
        if let Some(id) = source.take() {
            if !fired.get() {
                id.remove();
            }
        }
    }

    /// Huge-buffer robustness (mark placement + no-panic): the fresh far-restore
    /// must run its whole path on a 40k-line buffer without panicking or re-arming
    /// the GTK4Rs/AP-22 blank, and must anchor the persistent restore mark at the FAR
    /// target line (not mis-target it — the mark assert DOES fail under a pre-fix
    /// mutation, so it genuinely guards targeting). It does NOT prove the
    /// snap-to-top settling (see the module doc); that is MANUAL-TEST 3.2a.
    #[gtktest::test]
    fn fresh_far_restore_on_a_huge_buffer_targets_the_far_line_without_panicking() {
        let view = CodePreviewView::new();
        let body: String = (0..40_000).map(|n| format!("line {n}\n")).collect();
        view.buffer().set_text(&body);

        let sw = ScrolledWindow::new();
        sw.set_child(Some(&view));

        let target = 30_000;
        restore_preview_scroll_to_line_fresh(&sw, target);

        let buffer = view.buffer();
        let mark = buffer
            .mark("scrib-scroll-restore")
            .expect("fresh far-restore installs the persistent restore mark");
        assert_eq!(
            buffer.iter_at_mark(&mark).line(),
            target,
            "the restore mark must sit on the far target line, not be mis-targeted"
        );
    }

    /// Positive-path / no-wedge check (NOT the snap-to-top discriminator — see the
    /// module doc): on a real mapped view the fresh far-restore drives the viewport
    /// DOWN to the far target and off the top, without panicking or wedging. Because
    /// mapping+pumping validates heights first, this passes under both the fixed and
    /// the pre-fix restore — its value is proving the live path reaches the target
    /// and leaves input working, not distinguishing the snap-to-top regression.
    #[gtktest::test]
    fn fresh_far_restore_scrolls_a_mapped_view_to_the_far_target() {
        let view = CodePreviewView::new();
        let body: String = (0..4_000).map(|n| format!("line {n}\n")).collect();
        view.buffer().set_text(&body);
        let sw = ScrolledWindow::new();
        sw.set_child(Some(&view));

        let window = gtk::Window::new();
        window.set_default_size(500, 300);
        window.set_child(Some(&sw));
        window.present();

        let ctx = gtk::glib::MainContext::default();
        // Pump until the view is mapped + allocated (a display that never maps fails
        // loudly on the watchdog — reads as an env problem — instead of spinning).
        {
            let view = view.clone();
            let sw = sw.clone();
            pump_until(&ctx, "window never mapped/allocated", move || {
                view.is_mapped() && sw.vadjustment().upper() > 0.0
            });
        }

        let target = 2_000;
        restore_preview_scroll_to_line_fresh(&sw, target);

        // Pump until the viewport reaches the target (tolerance == the assert's).
        let vadj = sw.vadjustment();
        {
            let view = view.clone();
            let vadj = vadj.clone();
            pump_until(&ctx, "viewport never reached the target", move || {
                let (top, _) = view.line_at_y(vadj.value() as i32);
                top.line() >= target - 50
            });
        }

        let (top, _) = view.line_at_y(vadj.value() as i32);
        let top_line = top.line();
        let value = vadj.value();
        // Destroy BEFORE asserting so a failing run never leaks a mapped window.
        window.destroy();
        assert!(
            top_line >= target - 50,
            "fresh restore should scroll the mapped view near line {target}, got {top_line}"
        );
        assert!(value > 0.0, "viewport moved off the top");
    }

    /// ScrAP-152 regression, asserted on the STATE rather than the symptom.
    ///
    /// `restore_textview_scroll_to_line` defers its work to an idle. If that idle
    /// STRONG-captures the view, the view cannot finalize while the idle is
    /// pending — it survives as an unrooted zombie and the idle then drives
    /// `scroll_to_mark` against a torn-down surface (SIGSEGV). The symptom is a
    /// crash, which is a terrible thing to assert on; the *state* that causes it
    /// is "something other than the widget tree still holds a reference", and
    /// that is decidable here with no display, no pumping and no crash.
    ///
    /// Deliberately never realized: the pin is a property of the capture, not of
    /// the widget's lifecycle stage, so leaving it unrealized isolates the one
    /// thing under test. The only candidate owner left after the scroller drops
    /// its child is the pending idle closure.
    ///
    /// Mutation-tested: restoring the `view.clone()`/`sw.clone()` capture makes
    /// this fail (`upgrade()` returns `Some`).
    #[gtktest::test]
    fn the_deferred_restore_idle_does_not_pin_its_view_alive() {
        let view = TextView::new();
        view.buffer().set_text(&"line\n".repeat(200));
        let sw = ScrolledWindow::new();
        sw.set_child(Some(&view));

        restore_textview_scroll_to_line(&sw, &view, 150);

        let weak = view.downgrade();
        drop(view);
        sw.set_child(None::<&gtk::Widget>);
        drop(sw);

        assert!(
            weak.upgrade().is_none(),
            "the pending restore idle is still holding a strong reference to the \
             view — it will fire against an unrooted zombie after the window is \
             destroyed (ScrAP-152). Capture weakly."
        );
    }

    /// ScrAP-152's second half: `upgrade()` succeeding is NOT the guard.
    ///
    /// A zombie upgrades fine — a co-pending strong reference elsewhere keeps it
    /// upgradable — so the idle must ALSO ask whether the view is still realized
    /// before touching its geometry. This maps a real view, schedules the
    /// restore, destroys the window, and only then pumps: the scheduled work must
    /// decline to run.
    ///
    /// Note for whoever mutation-tests this: removing the `is_realized()` gate
    /// does not make this test FAIL so much as make the process ABORT inside GTK,
    /// which is the finding rather than a flaw in the test.
    #[gtktest::test]
    fn the_deferred_restore_idle_declines_to_run_against_a_destroyed_view() {
        let view = TextView::new();
        view.buffer().set_text(&"line\n".repeat(400));
        let sw = ScrolledWindow::new();
        sw.set_child(Some(&view));
        let window = gtk::Window::new();
        window.set_default_size(400, 300);
        window.set_child(Some(&sw));
        window.present();

        let ctx = gtk::glib::MainContext::default();
        {
            let view = view.clone();
            pump_until(&ctx, "the view never mapped", move || view.is_mapped());
        }

        restore_textview_scroll_to_line(&sw, &view, 300);
        window.destroy();
        // Drain the pending idle against the now-unrealized view.
        while ctx.pending() {
            ctx.iteration(false);
        }

        assert!(
            !view.is_realized(),
            "precondition: destroy() unrealizes synchronously"
        );
        assert_eq!(
            sw.vadjustment().value(),
            0.0,
            "the restore idle scrolled a destroyed view instead of declining"
        );
    }

    /// F-AP3-027: `restore_preview_scroll` must route a `CodePreviewView` through
    /// that subclass's guarded `scroll_to_buffer_offset`, not through the generic
    /// base-`TextView` fallback.
    ///
    /// It used to downcast straight to `TextView`, which a `CodePreviewView`
    /// satisfies — so the preview, the one view a mode switch destroys and
    /// rebuilds, silently skipped the coalescing, cancel-on-reschedule,
    /// weak-capture and realize gate that `schedule_scroll_idle` exists to
    /// provide. Its sibling `restore_preview_scroll_to_line` got this right, 160
    /// lines earlier in the same file.
    ///
    /// `reading_line()` is the observable: `scroll_to_buffer_offset` records the
    /// programmatic target in `restore_target_line` SYNCHRONOUSLY (only the scroll
    /// itself is deferred), and `reading_line()` returns that when set. Under the
    /// old routing it stays unset and `reading_line()` falls back to reading the
    /// live viewport, so the two paths return different lines — which is the
    /// discrimination, without reaching into `imp`.
    ///
    /// Mutation-tested: disabling the `CodePreviewView` arm gives 1000 against an
    /// expected 501. (1000 rather than 0 because the fallback reads
    /// `line_at_y(0)` on a view that has never been allocated; the number is not
    /// the point, the disagreement is.)
    #[gtktest::test]
    fn restore_preview_scroll_routes_a_code_preview_view_through_its_guarded_path() {
        let view = CodePreviewView::new();
        let body: String = (0..1_000).map(|n| format!("line {n}\n")).collect();
        view.buffer().set_text(&body);
        let sw = ScrolledWindow::new();
        sw.set_child(Some(&view));

        // 1000 body lines + the trailing empty line = 1001 → 0.5 rounds to 501.
        let lines = view.buffer().line_count();
        let expected = ((0.5 * lines as f64).round() as i32).clamp(0, lines - 1);

        restore_preview_scroll(&sw, 0.5);

        assert_eq!(
            view.reading_line(),
            expected,
            "the fraction restore did not go through CodePreviewView's guarded \
             scroll_to_buffer_offset — it took the unguarded base-TextView path"
        );
    }

    /// A `line <= 0` fresh-restore is a no-op (already at/above the top): it must
    /// NOT install a mark or touch the scroller, matching the one-shot restore's
    /// contract.
    #[gtktest::test]
    fn fresh_restore_is_a_noop_at_or_above_the_top() {
        let view = CodePreviewView::new();
        view.buffer().set_text("a\nb\nc\n");
        let sw = ScrolledWindow::new();
        sw.set_child(Some(&view));

        restore_preview_scroll_to_line_fresh(&sw, 0);
        assert!(
            view.buffer().mark("scrib-scroll-restore").is_none(),
            "line 0 is a no-op — no restore mark should be created"
        );
    }

    /// ScrAP-104 regression — the FATAL cross-buffer path. The
    /// progressive far-restore persists a mark on the buffer live at schedule time,
    /// then re-resolves it across many `notify::upper` passes via
    /// `view.buffer().iter_at_mark(&mark)` + `line_yrange`. If a `re_render`
    /// (`set_buffer`) swaps the view's buffer in between, the old buffer finalizes and
    /// ORPHANS the mark; `iter_at_mark` has NO mark∈buffer check in GTK 4.6
    /// (gtktextbuffer.c:2569 — unlike `scroll_to_mark`, which g_return_if_fails), so it
    /// returns an UNINITIALISED iter whose `line_yrange` aborts the process with
    /// `gtk_text_btree_line_number couldn't find line` (researcher-sourced). The
    /// cross-buffer guard bails on the orphaned mark. This maps the view, schedules the
    /// restore, swaps the buffer, and pumps — reaching the final assert proves the
    /// guarded closure survived resolving the orphaned mark.
    #[gtktest::test]
    fn a_progressive_restore_survives_a_buffer_swap_that_orphans_its_mark() {
        let view = CodePreviewView::new();
        let body_a: String = (0..4_000).map(|n| format!("alpha {n}\n")).collect();
        view.buffer().set_text(&body_a);
        let sw = ScrolledWindow::new();
        sw.set_child(Some(&view));

        let window = gtk::Window::new();
        window.set_default_size(500, 300);
        window.set_child(Some(&sw));
        window.present();

        let ctx = gtk::glib::MainContext::default();
        {
            let view = view.clone();
            let sw = sw.clone();
            pump_until(&ctx, "window never mapped/allocated", move || {
                view.is_mapped() && sw.vadjustment().upper() > 0.0
            });
        }

        // Schedule the progressive far-restore — installs the persistent mark on A.
        restore_preview_scroll_to_line_fresh(&sw, 2_000);
        let mark_a = view
            .buffer()
            .mark("scrib-scroll-restore")
            .expect("progressive restore installs the persistent mark on buffer A");

        // Swap to a DIFFERENT, equally-tall buffer so the follower's range stays > 0
        // (so the guarded `apply` actually reaches the iter_at_mark site) — finalizing
        // buffer A and orphaning the mark: the exact fatal condition.
        let buf_b = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
        let body_b: String = (0..4_000).map(|n| format!("beta {n}\n")).collect();
        buf_b.set_text(&body_b);
        view.set_buffer(Some(&buf_b));
        assert!(
            mark_a.buffer().is_none(),
            "set_buffer finalized buffer A → the restore mark is orphaned"
        );

        // Pump so the pending idle + notify::upper apply() fire under the guard. A
        // watchdog bounds the wait; reaching it without aborting is the assertion.
        let mut spins = 0;
        while ctx.pending() && spins < 2_000 {
            ctx.iteration(false);
            spins += 1;
        }
        window.destroy();
        assert_eq!(
            view.buffer(),
            buf_b,
            "the view still shows buffer B — the guarded restore never resolved the orphaned mark"
        );
    }
}
