//! Geometry & scrolling helpers for [`CodePreviewView`]: cache-free line/cell
//! buffer-Y reads (GTK4Rs/AP-22/ScrAP-105), the shared persistent-mark helper, and the
//! validation-safe scroll-to-offset / scroll-to-cell entry points. Split out of
//! the former monolithic `codeview.rs` (F-AP-002).

use super::CodePreviewView;
use gtk::glib;
use gtk::prelude::*;

/// Move a persistent, left-gravity `GtkTextMark` named `name` to `iter`, creating
/// it (left-gravity) on first use. Reusing ONE persistent mark — never a
/// create-then-delete each call — is a documented correctness rule: a
/// create+delete each scroll races GtkTextView's first-paragraph pinning and
/// leaves the view snapped to the top (GTK4Rs/AP-22/ScrAP-65). Shared by every
/// validation-safe scroll restore (`preview::scroll`'s one-shot and progressive
/// restores, and this view's own `scroll_to_buffer_offset`) so the idiom stays
/// byte-for-byte identical across all of them (QA M-5).
pub(crate) fn move_or_create_mark(
    buffer: &gtk::TextBuffer,
    name: &str,
    iter: &gtk::TextIter,
) -> gtk::TextMark {
    match buffer.mark(name) {
        Some(m) => {
            buffer.move_mark(&m, iter);
            m
        }
        None => buffer.create_mark(Some(name), iter, true),
    }
}

/// Buffer-y of the TOP of the line containing char `offset`, read via `line_yrange`
/// (a pure cached-btree-height read) — NEVER `iter_location`. Painted extents in
/// `snapshot_layer` must be cache-free: `iter_location` builds+caches a line DISPLAY
/// and inserts it into the view's line-display GSequence (kept sorted by line number);
/// immediately after a `set_buffer` swap that cache still holds displays whose
/// `GtkTextLine`s were freed with the old buffer, so the insert's comparator calls
/// `_gtk_text_line_get_number` on a freed line → SIGSEGV / `gtk_text_btree_line_number
/// couldn't find line` (ScrAP-105). `line_yrange` touches no display cache, so
/// it is immune, and on a validated on-screen line returns the same top-y as
/// `iter_location`.
pub(crate) fn line_top_y(
    view: &impl IsA<gtk::TextView>,
    buffer: &gtk::TextBuffer,
    offset: i32,
) -> i32 {
    view.line_yrange(&buffer.iter_at_offset(offset)).0
}

/// Buffer-y of the BOTTOM (top + height) of the line containing char `offset`,
/// cache-free — see [`line_top_y`] (ScrAP-105).
pub(crate) fn line_bottom_y(
    view: &impl IsA<gtk::TextView>,
    buffer: &gtk::TextBuffer,
    offset: i32,
) -> i32 {
    let (y, h) = view.line_yrange(&buffer.iter_at_offset(offset));
    y + h
}

/// Vertical `(top, bottom)` buffer-Y extent of a `span`'s self-drawn decoration (a
/// code-block card, a blockquote accent bar), clamped to the viewport edges so no
/// off-screen (unvalidated) iter is ever measured mid-paint (GTK4Rs/AP-22): a boundary whose
/// line is on-screen is read via cache-free `line_top_y`/`line_bottom_y`; a boundary
/// past the viewport edge clamps to that edge instead.
///
/// The extent is EXACTLY the span's own line range —
/// `[line_top_y(start), line_bottom_y(last_content)]` — with **no extra padding**. A
/// code block's inner vertical padding is already inside those lines' `line_yrange`
/// (the `code-block-top`/`code-block-bottom` `pixels_above/below_lines` tags), so
/// adding a further pad here double-counted it AND, on the bottom, reached past the
/// block's last line and bled the card onto the immediately-following line whenever no
/// blank block-separator absorbed it — a loose continuation paragraph abutting a code
/// block inside a list item (GTK4Rs/AP-127). Shared by the code-block
/// and blockquote loops in `snapshot_layer` (identical clamp) and exercised directly by
/// the regression test, so the paint path and the test read the SAME formula (GTK4Rs/AP-78).
#[allow(clippy::too_many_arguments)]
pub(crate) fn span_card_y_extent(
    view: &impl IsA<gtk::TextView>,
    buffer: &gtk::TextBuffer,
    span: crate::span::BufferSpan,
    vis_start: i32,
    vis_end: i32,
    vtop: f32,
    vbot: f32,
) -> (f32, f32) {
    let top = if span.start >= vis_start {
        line_top_y(view, buffer, span.start) as f32
    } else {
        vtop
    };
    let bottom = if span.last_content() <= vis_end {
        line_bottom_y(view, buffer, span.last_content()) as f32
    } else {
        vbot
    };
    (top, bottom)
}

/// `(buffer-Y, height)` — in the f32 buffer space `snapshot_layer` paints in — of a
/// table CELL's own row: `table-top + cell-within-table`. Both halves are scroll- AND
/// placeholder-immune (bug C / GTK4Rs/AP-91): `gtk_text_view_allocate_children`
/// parks every anchored child off-screen (x=-w, y=-h, correct SIZE but WRONG POSITION,
/// gtktextview.c:4442) until its line validates, so
///  • table-top = `line_yrange` of the TABLE's OWN child-anchor iter — a cache-free
///    btree read that touches no allocation (NOT the cell→view translate, which reads
///    the poisoned origin → wrong-but-nonzero Y = the stale-chip bug; NOT
///    `get_iter_location`, which VALIDATES the line mid-snapshot → blank view, GTK4Rs/AP-22);
///  • cell offset = cell→TABLE translate, a LOCAL subtree transform that never touches
///    the view-relative (poisoned) origin.
///
/// Falls back to the table's own row `(table_top, table_height)` while the cell
/// transform is unavailable (still-settling / off-screen placeholder) or lands outside
/// the table's extent. Shared single source for the marker-chip refine
/// (`snapshot_layer`) and find's cell scroll-to-hit ([`refine_scroll_to_cell`]) —
/// `preview::cells` was flagged for reuse here.
pub(crate) fn cell_row_y_h(
    view: &impl IsA<gtk::TextView>,
    buffer: &gtk::TextBuffer,
    tanchor: &gtk::TextChildAnchor,
    label: &gtk::Label,
) -> (f32, f32) {
    let (ty, th) = view.line_yrange(&buffer.iter_at_child_anchor(tanchor));
    let mut y = ty as f32;
    let mut h = th as f32;
    if let Some(table) = label.ancestor(crate::widgets::table::ScribTableWidget::static_type()) {
        if let Some((_cx, cy)) = label.translate_coordinates(&table, 0.0, 0.0) {
            let ch = label.height();
            // Sanity band: the cell offset must land within the table's own extent
            // (guards a still-settling transform; a positive height alone is NOT
            // enough — the off-screen placeholder has a size too).
            if ch > 0 && cy >= 0.0 && cy as i32 <= table.height() {
                y = ty as f32 + cy as f32;
                h = ch as f32;
            }
        }
    }
    (y, h)
}

/// Buffer-space `(y, height)` of ONE comment marker's chip row: the marker's anchor
/// line, refined to the exact table row when the marker belongs to a table cell.
///
/// **The single source of truth for where a chip sits vertically**, shared by the
/// painter (`snapshot_layer(AboveText)`, which draws the chip) and by the annotation
/// card (which anchors itself to that chip). They MUST agree: the card points at the
/// chip, so a second implementation is free to drift and the drift is visible as a
/// card floating away from the chip it belongs to. Same shared-extent-helper reflex as
/// [`span_card_y_extent`] (GTK4Rs/AP-78/GTK4Rs/AP-127) — one formula, two callers, one test.
///
/// Cache-free by construction: the base row is [`gtk::prelude::TextViewExt::line_yrange`]
/// (a btree height read that touches no line-display cache — GTK4Rs/AP-22/ScrAP-105), never
/// `iter_location`. The cell refinement delegates to the placeholder-immune
/// [`cell_row_y_h`] (GTK4Rs/AP-91); while a table's cells have not allocated it falls back
/// to the table's own top, so the chip stays visible and snaps to the exact row on a
/// later frame.
pub(crate) fn marker_row_y_h(
    view: &impl IsA<gtk::TextView>,
    buffer: &gtk::TextBuffer,
    m: &super::MarkerData,
) -> (f32, f32) {
    // Base Y: the buffer line of the marker's anchor (the table's U+FFFC for a cell
    // claim; the body line for a prose claim).
    let (my, mh) = view.line_yrange(&buffer.iter_at_offset(m.anchor));
    let Some(tanchor) = &m.cell_table_anchor else {
        return (my as f32, mh as f32);
    };
    match m.cell_widget.as_ref().and_then(|w| w.upgrade()) {
        Some(label) => cell_row_y_h(view, buffer, tanchor, &label),
        // Weak cell ref dead (pre-allocation): the table's own top row directly.
        None => {
            let (ty, th) = view.line_yrange(&buffer.iter_at_child_anchor(tanchor));
            (ty as f32, th as f32)
        }
    }
}

/// The chip rectangle for a marker row, in the view's reserved RIGHT-MARGIN column —
/// `(x, y, width, height)` with `y` still in **buffer** space (the caller converts to
/// widget space with `buffer_to_window_coords` when it needs a hit-box or a popover
/// anchor).
///
/// Pure arithmetic over plain numbers, so it is unit-testable without a display — the
/// other half of the [`marker_row_y_h`] sharing. The chip is a fixed-ish glyph centred
/// vertically on its row: a tall row (a table cell, a wrapped paragraph) gets the chip
/// in the middle rather than a chip stretched down the whole row.
pub(crate) fn chip_rect(
    view_width: f32,
    right_margin: f32,
    row_y: f32,
    row_h: f32,
) -> (f32, f32, f32, f32) {
    let w = (right_margin - 4.0).clamp(6.0, 14.0);
    let x = view_width - right_margin + 2.0;
    let h = row_h.clamp(8.0, 18.0);
    let y = row_y + (row_h - h) / 2.0;
    (x, y, w, h)
}

/// Step-2 refine of [`CodePreviewView::scroll_to_cell_offset`]: nudge `view`'s
/// vadjustment so table `label`'s own row sits at the viewport top (matching the
/// `yalign 0.0` a body hit gets). Resolves the row through the shared, placeholder-
/// immune [`cell_row_y_h`]; while the cell transform is still unavailable that yields
/// the table top, so this lands on the same place step 1 already scrolled to (a safe
/// no-op) and a later navigation step re-refines once the cells allocate.
fn refine_scroll_to_cell(
    view: &CodePreviewView,
    tanchor: &gtk::TextChildAnchor,
    label: &gtk::Label,
) {
    let buffer = view.buffer();
    let (cell_y, _h) = cell_row_y_h(view, &buffer, tanchor, label);
    let Some(vadj) = view.vadjustment() else {
        return;
    };
    let max = (vadj.upper() - vadj.page_size()).max(vadj.lower());
    crate::saferizer::scrollpos::jump(&vadj, (cell_y as f64).clamp(vadj.lower(), max));
}

impl CodePreviewView {
    /// Schedule `work` on the next main-loop idle, guarded against the
    /// deferred-idle-outlives-the-view SIGSEGV class (ScrAP-152 / GTK4Rs/AP-63).
    /// This is the ONE place the discipline every deferred view-scroll needs is
    /// spelled out, so a new call site cannot silently omit a facet of it:
    ///
    /// 1. **Coalesce** — cancel any idle already pending in `scroll_idle` (only the
    ///    latest target survives; no stacked scrolls re-triggering validation churn,
    ///    GTK4Rs/AP-22).
    /// 2. **Weak capture** — the view is captured weakly, never a strong clone. A
    ///    strong capture would pin the view alive as an unrooted *zombie* after
    ///    `window.destroy()` unrealizes-but-does-not-finalize the subtree
    ///    (gtkwindow.c:6717-6744), and the still-pending idle would fire against it.
    /// 3. **Clear the slot FIRST** — on fire, `scroll_idle` is set to `None` before
    ///    anything else, because a fired one-shot's `SourceId` must never be
    ///    `.remove()`d (glib 0.21.5 panics on an already-fired id, source.rs:34-46)
    ///    and ids are recycled — so `unrealize`/coalescing must never `.remove()`
    ///    this now-fired source.
    /// 4. **`is_realized()` gate** — `work` runs ONLY if the view is still realized.
    ///    `upgrade()` is liveness-only (a zombie is alive-but-unrealized), so the
    ///    real precondition — a `scroll_to_mark` on an unrealized view drives a
    ///    popover realize whose parent surface is NULL → `GDK_IS_SURFACE` assert
    ///    (gtkpopover.c:944-947 / gdksurface.c:885) — is gated on `is_realized()`,
    ///    the exact rooted/realized test. NOT the stricter `is_mapped()`: a
    ///    realized-but-unmapped fresh tab (fragment-link scroll) is still realized
    ///    and must NOT be skipped.
    ///
    /// `work` may itself call `schedule_scroll_idle` (same slot) to chain a follow-up
    /// pass (the cell-scroll two-step) — the step-3 slot-clear makes that re-entry
    /// safe. Pairs with the synchronous cancel in `unrealize` (codeview/mod.rs):
    /// that facet + this closure guard are defense-in-depth, either alone preventing
    /// the crash (the sourced verdict).
    pub(crate) fn schedule_scroll_idle(&self, work: impl FnOnce(&CodePreviewView) + 'static) {
        use gtk::subclass::prelude::*;
        if let Some(id) = self.imp().scroll_idle.borrow_mut().take() {
            id.remove();
        }
        let id = glib::idle_add_local_once(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move || {
                view.imp().scroll_idle.replace(None);
                if !view.is_realized() {
                    return;
                }
                work(&view);
            }
        ));
        self.imp().scroll_idle.replace(Some(id));
    }

    /// Scroll so the buffer position `offset` is at the top of the viewport,
    /// coalescing rapid re-targeting (e.g. fast outline clicks) into a single
    /// scroll of the latest target.
    ///
    /// Uses one persistent left-gravity mark + `scroll_to_mark` (deferred until
    /// line validation) on an idle, NOT `scroll_to_iter` (immediate, pre-validation
    /// — blanks the view). Coalescing avoids stacking N in-flight scrolls, each of
    /// which would re-trigger validation churn (GTK4Rs/AP-22).
    pub(crate) fn scroll_to_buffer_offset(&self, offset: i32) {
        use gtk::subclass::prelude::*;
        let buffer = self.buffer();
        let it = buffer.iter_at_offset(offset);
        // A programmatic scroll takes over from any hand-scrolling: record the
        // target as the cached anchor and stop tracking value-changed, so this
        // scroll's own animation cannot overwrite the anchor mid-burst (ScrAP-65).
        self.imp().user_scrolling.set(false);
        self.imp().restore_target_line.set(Some(it.line()));
        let mark = move_or_create_mark(&buffer, "scrib-outline-target", &it);
        // Pair the persisted mark with its owning buffer so the deferred idle can
        // gate resolution on membership (ScrAP-104) via the one safe path.
        let bmark = crate::saferizer::buffer_mark::BufferMark::new(mark, &buffer);
        // The deferred-idle discipline (coalesce + weak capture + slot-clear-first +
        // is_realized gate — ScrAP-152) lives in `schedule_scroll_idle`.
        self.schedule_scroll_idle(move |view| {
            // Cross-buffer guard (ScrAP-104): if a `set_buffer` (zoom / live-edit
            // re_render) swapped this view's buffer between scheduling and now, the old
            // buffer may have finalized and orphaned the mark. `scroll_to_mark` resolves
            // the mark against the view's CURRENT buffer via the same unchecked
            // `get_iter_at_mark` path — a foreign/orphaned mark drives it into a garbage
            // iter → fatal "gtk_text_btree_line_number couldn't find line". The seam withholds
            // the mark (None) unless it still belongs to the live buffer, so we bail then.
            let Some(mark) = bmark.scroll_mark(&view.buffer()) else {
                return;
            };
            // A FAR scroll_to_mark in a long buffer must not race GtkTextView's
            // lazy line-height validator — the total height keeps changing across
            // idle passes, re-queuing a resize each frame, so paint bails
            // ("snapshot without a current allocation" → blank view + unpainted
            // scrollbar) until it converges (GTK4Rs/AP-22). The single force that makes
            // the far scroll land on a stable allocation is `scroll_to_mark`'s own
            // internal `flush_scroll` → `validate_yrange`. A prior `line_yrange(end)`
            // pre-read used to sit here as a supposed validation-primer; the researcher
            // confirmed against GTK 4.6.9 that `gtk_text_view_get_line_yrange` runs NO
            // validation on ANY path (pure cached-height btree read), so that call was
            // vestigial and has been removed (ANTI-PATTERNS deferred-work meta-pattern,
            // myth-bust #1). yalign 0.0 puts the heading at the top of the viewport.
            #[allow(clippy::disallowed_methods)] // deliberate raw call — see clippy.toml
            view.scroll_to_mark(mark, 0.0, true, 0.0, 0.0);
        });
    }

    /// Re-anchor the viewport to the user's tracked reading line after a GEOMETRY
    /// change (a width change re-wraps the text) — the restore half of the
    /// Reading-Position Preservation CAM's geometry path (row 7). The buffer is
    /// unchanged across a resize, so the reading line ([`reading_line`](Self::reading_line),
    /// maintained continuously by `wire_scroll_position_tracking`) is still valid and
    /// already captured; this only restores it, top-aligned.
    ///
    /// Routes through the same coalesced, deferred, weak-captured, `is_realized`-gated
    /// [`scroll_to_buffer_offset`](Self::scroll_to_buffer_offset) — NEVER a one-shot
    /// adjustment `set_value`, which the lazy-validation `upper` clamp resets straight
    /// back toward the top (GTK4Rs/AP-13/65). A no-op at/above the top (the clamp already
    /// lands there) and if the line cannot be resolved.
    pub(crate) fn reanchor_to_reading_line(&self) {
        let line = self.reading_line();
        if line <= 0 {
            return;
        }
        let Some(it) = self.buffer().iter_at_line(line) else {
            return;
        };
        self.scroll_to_buffer_offset(it.offset());
    }

    /// Scroll a table-**cell** match to its own row, not merely the table's top.
    /// A whole table is a single `GtkTextChildAnchor` (one
    /// buffer line); its cell text lives in anchored `GtkLabel`s OUTSIDE the buffer,
    /// so a plain [`scroll_to_buffer_offset`](Self::scroll_to_buffer_offset) on the
    /// anchor only ever reaches the table top — every cell match in one table maps to
    /// the SAME offset, so the coalesced scroll is a no-op and the viewport looks
    /// "stuck" (the DDD symptom) across in-cell matches.
    ///
    /// Two-step, reusing the marker cell→table geometry (bug C / GTK4Rs/AP-91):
    /// 1. `scroll_to_mark` the table's own child anchor — brings the table into view
    ///    AND forces GtkTextView to validate + allocate the table and its cell labels
    ///    (an anchored child is parked off-screen with the WRONG position until its
    ///    line validates, gtktextview.c:4442), so the step-2 geometry is available.
    /// 2. On the NEXT idle (allocation settled), [`refine_scroll_to_cell`] nudges the
    ///    vadjustment to the cell's own buffer-Y (table-top + cell-within-table),
    ///    landing that row at the viewport top — the same top-align a body hit gets
    ///    from `yalign 0.0`.
    ///
    /// Shares the `scrib-outline-target` mark and `scroll_idle` slot with
    /// `scroll_to_buffer_offset`, so a later navigation step cancels this one's
    /// pending idle (only the last Next/Prev survives — no stacked nudges).
    pub(crate) fn scroll_to_cell_offset(&self, anchor_off: i32, label: &gtk::Label) {
        use gtk::subclass::prelude::*;
        let buffer = self.buffer();
        let it = buffer.iter_at_offset(anchor_off);
        let Some(tanchor) = it.child_anchor() else {
            // The offset is not actually a child anchor — fall back to a plain scroll.
            self.scroll_to_buffer_offset(anchor_off);
            return;
        };
        // Step 1: mirror scroll_to_buffer_offset's coalesced, validation-safe
        // scroll_to_mark (GTK4Rs/AP-22/ScrAP-65/ScrAP-104), then refine to the exact cell row.
        self.imp().user_scrolling.set(false);
        self.imp().restore_target_line.set(Some(it.line()));
        let mark = move_or_create_mark(&buffer, "scrib-outline-target", &it);
        // Pair with the owning buffer for the membership-gated resolve (ScrAP-104).
        let bmark = crate::saferizer::buffer_mark::BufferMark::new(mark, &buffer);
        // Both the outer idle and the nested inner one below carry the ANTI-PATTERNS
        // ScrAP-152 discipline (coalesce + weak + slot-clear-first + is_realized) via
        // `schedule_scroll_idle` — the inner re-uses the same slot, and the outer's
        // step-3 slot-clear makes that chained re-schedule safe.
        let label = label.clone();
        self.schedule_scroll_idle(move |view| {
            // Cross-buffer guard (ScrAP-104): a set_buffer between scheduling and now
            // orphans the mark; scroll_to_mark on a foreign mark aborts fatally. The
            // seam withholds the mark unless it still belongs to the live buffer.
            let Some(mark) = bmark.scroll_mark(&view.buffer()) else {
                return;
            };
            #[allow(clippy::disallowed_methods)] // deliberate raw call — see clippy.toml
            view.scroll_to_mark(mark, 0.0, true, 0.0, 0.0);
            // Step 2: refine to the cell's own row once the table (forced to validate
            // by scroll_to_mark) has allocated its cells on the next idle pass — chained
            // through the same guarded sink so `unrealize`/coalescing cancel it too
            // (cancelling the OUTER on unrealize does not cover an inner already
            // scheduled when unrealize hits).
            let label = label.clone();
            view.schedule_scroll_idle(move |view| {
                refine_scroll_to_cell(view, &tanchor, &label);
            });
        });
    }
}

/// Regression coverage for ScrAP-152 — a deferred scroll idle outliving the view it
/// scrolls (a measured, deterministic SIGSEGV). Both tests need a real `#[gtktest::test]`
/// main loop (GTK4Rs/AP-71), so they live behind `gtk-integration-tests`.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;
    use gtk::ScrolledWindow;

    // The pump-until-or-panic helper that used to live here now lives at
    // `crate::testpump::until` under `Clock::Idle` (M31) — this file's copy and the
    // byte-identical one in `preview/scroll.rs` are the two it replaces.

    /// A realized+mapped `CodePreviewView` over an `n`-line buffer, with its scroller,
    /// window, and the shared main context. Pumps until the view is mapped and the
    /// vadjustment has a real upper (a display that never maps fails loudly on the
    /// watchdog rather than spinning).
    fn mapped_view(
        n: usize,
    ) -> (
        CodePreviewView,
        ScrolledWindow,
        gtk::Window,
        gtk::glib::MainContext,
    ) {
        let view = CodePreviewView::new();
        let body: String = (0..n).map(|i| format!("line {i}\n")).collect();
        view.buffer().set_text(&body);
        let sw = ScrolledWindow::new();
        sw.set_child(Some(&view));
        let window = gtk::Window::new();
        window.set_default_size(400, 300);
        window.set_child(Some(&sw));
        window.present();
        let ctx = gtk::glib::MainContext::default();
        {
            let view = view.clone();
            let sw = sw.clone();
            crate::testpump::until(
                crate::testpump::Clock::Idle,
                "window never mapped/allocated",
                move || view.is_mapped() && sw.vadjustment().upper() > 0.0,
            );
        }
        (view, sw, window, ctx)
    }

    /// The crash guard (ScrAP-152). A scroll is requested, then the view is DESTROYED
    /// before the idle fires — the exact rapid schedule-then-close shape. Pre-fix, the
    /// strong-capturing idle pins the view alive as an unrooted zombie and fires
    /// `scroll_to_mark` against it → the popover-realize SIGSEGV. Post-fix, `unrealize`
    /// cancels the pending source synchronously at `destroy()` AND the closure
    /// weak-captures + `is_realized()`-gates — so the pump completes cleanly. Reaching
    /// the end without aborting IS the pass. Mutation note: this aborts only when the
    /// whole ScrAP-152 fix is reverted (either facet alone still prevents the crash —
    /// that is the intended defense-in-depth, per the sourced verdict).
    #[gtktest::test]
    fn a_pending_scroll_idle_does_not_crash_after_the_view_is_destroyed() {
        let (view, sw, window, ctx) = mapped_view(600);
        let far = view
            .buffer()
            .iter_at_line(500)
            .map(|it| it.offset())
            .expect("line 500 exists in a 600-line buffer");
        assert_ne!(far, 0, "sanity: line 500 is not offset 0");

        // Schedule the deferred scroll, then tear the view down in the SAME turn with no
        // pump — leaving the idle pending against a now-unrealized view.
        view.scroll_to_buffer_offset(far);
        window.destroy();

        // Drop every strong ref this test holds. Pre-fix the idle's strong clone would
        // still pin the view alive; post-fix nothing does, so the view finalizes here.
        drop(view);
        drop(sw);
        drop(window);

        // Fire whatever is still pending. Must not abort.
        while ctx.pending() {
            ctx.iteration(false);
        }
    }

    /// GTK4Rs/AP-22 / no-regression: the `is_realized()` crash guard must NOT block a scroll on
    /// a live, realized, mapped view — the normal outline/find/fragment path. The
    /// viewport must actually move off the top toward the far target. (This is also why
    /// the gate is `is_realized()`, not the stricter `is_mapped()`: skipping a realized
    /// view's scroll would break the fresh-tab fragment scroll of `scroll_preview_to_fragment`.)
    #[gtktest::test]
    fn a_scroll_on_a_mapped_view_still_reaches_the_far_target() {
        let (view, sw, window, _ctx) = mapped_view(2_000);
        let target_line = 1_500;
        let far = view
            .buffer()
            .iter_at_line(target_line)
            .map(|it| it.offset())
            .expect("line 1500 exists in a 2000-line buffer");

        view.scroll_to_buffer_offset(far);

        let vadj = sw.vadjustment();
        {
            let view = view.clone();
            let vadj = vadj.clone();
            crate::testpump::until(
                crate::testpump::Clock::Idle,
                "viewport never reached the far target",
                move || {
                    let (top, _) = view.line_at_y(vadj.value() as i32);
                    top.line() >= target_line - 50
                },
            );
        }
        let (top, _) = view.line_at_y(vadj.value() as i32);
        let top_line = top.line();
        let value = vadj.value();
        // Destroy BEFORE asserting so a failing run never leaks a mapped window.
        window.destroy();
        assert!(
            top_line >= target_line - 50,
            "the mapped-view scroll should reach ~line {target_line}, got {top_line}"
        );
        assert!(value > 0.0, "the viewport moved off the top");
    }

    /// The `schedule_scroll_idle` sink COALESCES: two rapid schedules leave only the
    /// latest pending (the earlier source is cancelled), so exactly one — the last —
    /// runs. This is the GTK4Rs/AP-22 "no stacked scrolls" guarantee, isolated from the
    /// scroll work. Mutation-guards the cancel-pending facet: drop the sink's
    /// `take().remove()` and BOTH closures run (`[1, 2]`), failing this. (The
    /// runs-when-realized and crash-after-destroy facets are covered by the two
    /// tests above.)
    #[gtktest::test]
    fn schedule_scroll_idle_coalesces_to_the_latest() {
        use std::cell::RefCell;
        use std::rc::Rc;
        let (view, _sw, window, ctx) = mapped_view(50);
        let ran: Rc<RefCell<Vec<u32>>> = Rc::new(RefCell::new(Vec::new()));

        view.schedule_scroll_idle({
            let r = ran.clone();
            move |_| r.borrow_mut().push(1)
        });
        view.schedule_scroll_idle({
            let r = ran.clone();
            move |_| r.borrow_mut().push(2)
        });

        {
            let ran = ran.clone();
            crate::testpump::until(
                crate::testpump::Clock::Idle,
                "the coalesced idle never ran",
                move || !ran.borrow().is_empty(),
            );
        }
        // Give any (wrongly) surviving earlier source a chance to also fire.
        while ctx.pending() {
            ctx.iteration(false);
        }

        window.destroy();
        assert_eq!(
            &*ran.borrow(),
            &[2],
            "only the latest-scheduled work runs — the earlier idle was coalesced away"
        );
    }
}

#[cfg(test)]
mod chip_geometry_tests {
    use super::chip_rect;

    /// A chip is CENTRED on its row, never stretched down it.
    ///
    /// This is the half of the marker geometry that can be tested without a display, and
    /// it is worth pinning precisely because the other half (`marker_row_y_h`) cannot be:
    /// the painter and the annotation card now share both, so an arithmetic change here
    /// moves the drawn chip and the card that points at it together. A test that only
    /// exercised the painter would let the two drift apart again.
    #[test]
    fn a_chip_is_centred_on_a_tall_row() {
        // A 60px-tall row (a table cell) gets an 18px chip centred in it, not a 60px bar.
        let (_x, y, _w, h) = chip_rect(800.0, 24.0, 100.0, 60.0);
        assert_eq!(h, 18.0, "chip height is clamped, not the row's");
        assert_eq!(y, 100.0 + (60.0 - 18.0) / 2.0, "centred within the row");
    }

    /// A short row keeps the chip inside it rather than overflowing above and below.
    #[test]
    fn a_short_row_gets_a_chip_no_taller_than_the_floor() {
        let (_x, y, _w, h) = chip_rect(800.0, 24.0, 100.0, 4.0);
        assert_eq!(h, 8.0, "chip height floors at 8");
        // Floored height exceeds the row, so the centring offset is negative — the chip
        // straddles the row's midline, which is what "centred" must keep meaning.
        assert_eq!(y, 100.0 + (4.0 - 8.0) / 2.0);
    }

    /// An ordinary text line takes the row height verbatim (inside the clamp band), so the
    /// chip lines up with the line it annotates.
    #[test]
    fn an_ordinary_line_uses_its_own_height() {
        let (_x, y, _w, h) = chip_rect(800.0, 24.0, 240.0, 17.0);
        assert_eq!(h, 17.0);
        assert_eq!(y, 240.0, "no vertical offset when the chip fills the row");
    }

    /// The chip sits in the RESERVED right-margin column — the whole reason the preview
    /// turns overlay scrolling off and reserves that column (a floating scrollbar over it
    /// would eat the chip's clicks).
    #[test]
    fn the_chip_sits_in_the_right_margin_column() {
        let (x, _y, w, _h) = chip_rect(800.0, 24.0, 0.0, 17.0);
        assert_eq!(
            x,
            800.0 - 24.0 + 2.0,
            "inset into the margin, not over the text"
        );
        assert!(
            x + w <= 800.0,
            "the chip must not overhang the view's right edge"
        );
    }

    /// A narrow or absurdly wide margin still yields a sane chip: the width is clamped at
    /// both ends, so a theme with a thin margin cannot produce a sub-pixel target and one
    /// with a huge margin cannot produce a slab.
    #[test]
    fn chip_width_is_clamped_at_both_ends() {
        let (_x, _y, narrow, _h) = chip_rect(800.0, 2.0, 0.0, 17.0);
        assert_eq!(narrow, 6.0, "floor");
        let (_x, _y, wide, _h) = chip_rect(800.0, 200.0, 0.0, 17.0);
        assert_eq!(wide, 14.0, "ceiling");
    }
}
