//! Pure, GTK-free geometry and decision logic for the tab strip
//! ([`super::TabBar`]).
//!
//! Extracted from the widget's `size_allocate` / hit-test / reorder / animation
//! methods so the load-bearing arithmetic — how the chevron gutters are
//! reserved, which tab a point hits, where a dragged tab reslots, how the
//! sibling-slide easing steps — is **unit-testable without a live display**
//! (POLICY §coverage gate: "extract a pure decision core into a logic module and
//! unit-test it"), mirroring [`crate::widgets::table::layout`]. The only GTK the
//! strip still owns is the `measure()` calls that feed the per-tab widths in
//! here; everything downstream of those measurements is plain arithmetic.

/// Pixel slack below which a sub-pixel overflow / scroll delta is treated as
/// "none" — stops a chevron flickering on for a rounding-error overflow, and
/// stops `show_prev`/`show_next` toggling on a fractional scroll residue.
const OVERFLOW_EPS: f64 = 0.5;

/// Distance (px) below which an animating tab handle is considered to have
/// reached its target and the per-frame slide can settle.
const SETTLE_EPS: f64 = 0.5;

/// The `GtkAdjustment` step / page increments, as a fraction of the viewport
/// width (a wheel notch nudges ~10%, a page ~90%), floored at [`MIN_STEP`] so a
/// zero-width viewport still yields a positive, valid increment.
const STEP_FRACTION: f64 = 0.1;
const PAGE_FRACTION: f64 = 0.9;
const MIN_STEP: f64 = 1.0;

/// How far outside `[0, width)` a currently-hidden chevron is shoved so the
/// strip's `Overflow::Hidden` clips it from the paint (ScrAP-56). Any
/// positive margin works; this keeps it comfortably clear of the edge.
pub(super) const OFFSCREEN_MARGIN: i32 = 4;

/// One tab handle's span in logical (content) x-space: its left origin and its
/// measured width. Named fields (not a bare `(f64, f64)`) so hit-testing and
/// reorder read unambiguously at the call site.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Span {
    pub(super) start: f64,
    pub(super) width: f64,
}

/// Result of reserving chevron gutters within the strip's allocated width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Gutter {
    /// Width reserved on EACH side for a chevron (0 when none are shown).
    pub(super) reserved: i32,
    /// Width left for the tab handles between the two gutters.
    pub(super) viewport_w: i32,
}

/// Decide the chevron gutter reservation and the resulting tab viewport width,
/// given the strip's allocated `width`, the full logical content width
/// `full_content_w` (as if the chevrons reserved nothing), and the chevrons'
/// true natural `chevron_w`. Chevrons are only reserved once the strip actually
/// overflows AND there is comfortably room for both at their true size — an
/// early/narrow layout pass skips them rather than forcing an undersized,
/// negative-width chevron allocation (see `imp::TabBar::size_allocate`).
pub(super) fn resolve_gutter(width: i32, full_content_w: f64, chevron_w: i32) -> Gutter {
    let overflow_at_all = full_content_w > width as f64 + OVERFLOW_EPS;
    let can_show_chevrons = width >= 2 * chevron_w;
    let reserved = if overflow_at_all && can_show_chevrons {
        chevron_w
    } else {
        0
    };
    let viewport_w = (width - 2 * reserved).max(0);
    Gutter {
        reserved,
        viewport_w,
    }
}

/// The scrollable extent for a `GtkAdjustment`: its `upper` (the full logical
/// content width, never less than the viewport so a non-overflowing strip
/// simply can't scroll) and the maximum scroll offset `max_off`.
pub(super) fn scroll_extent(full_content_w: f64, viewport_w: f64) -> (f64, f64) {
    let upper = full_content_w.max(viewport_w);
    let max_off = (upper - viewport_w).max(0.0);
    (upper, max_off)
}

/// The adjustment's step and page increments for a given viewport width.
pub(super) fn page_steps(viewport_w: f64) -> (f64, f64) {
    let step = (viewport_w * STEP_FRACTION).max(MIN_STEP);
    let page = (viewport_w * PAGE_FRACTION).max(MIN_STEP);
    (step, page)
}

/// Whether the prev / next chevrons should be shown, given the reserved gutter
/// (0 ⇒ neither, the strip doesn't overflow) and the current vs maximum scroll
/// offset. Returned as a named-destructurable `(show_prev, show_next)`.
pub(super) fn chevron_visibility(reserved: i32, off: f64, max_off: f64) -> (bool, bool) {
    let show_prev = reserved > 0 && off > OVERFLOW_EPS;
    let show_next = reserved > 0 && off < max_off - OVERFLOW_EPS;
    (show_prev, show_next)
}

/// Hit-test a logical (content-space) x against the tab handle `spans`. Returns
/// the index of the handle whose `[start, start + width)` span contains
/// `logical_x`, or `None`. The caller is responsible for rejecting points in a
/// chevron gutter *before* converting them to logical space (the gutters belong
/// to the chevrons, not to any scrolled-under tab).
pub(super) fn hit_index(logical_x: f64, spans: &[Span]) -> Option<usize> {
    spans
        .iter()
        .position(|s| logical_x >= s.start && logical_x < s.start + s.width)
}

/// Compute the insertion index for a dragged tab, given the hover x in logical
/// (content) space, the dragged tab's current index `from`, and every tab's
/// span (using its *target* x). Counts how many OTHER tabs' midpoints lie left
/// of the hover point — an insertion-sort-style position equivalent to the
/// retired tab-widget plan §A's clamp-and-compare recipe.
pub(super) fn reorder_index(logical_x: f64, from: usize, spans: &[Span]) -> usize {
    let mut new_index = 0usize;
    for (i, s) in spans.iter().enumerate() {
        if i == from {
            continue;
        }
        let mid = s.start + s.width / 2.0;
        if logical_x > mid {
            new_index += 1;
        }
    }
    new_index
}

/// The new scroll value needed to bring a handle at `pos` of width `w` fully
/// into a `viewport`-wide window currently scrolled to `value`, or `None` if it
/// is already fully visible (scroll left to reveal it, or right by the overshoot).
pub(super) fn scroll_target(pos: f64, w: f64, value: f64, viewport: f64) -> Option<f64> {
    if pos < value {
        Some(pos.max(0.0))
    } else if pos + w > value + viewport {
        Some((pos + w - viewport).max(0.0))
    } else {
        None
    }
}

/// The per-frame exponential-ease factor `k = 1 - e^(-dt/tau)` for a frame
/// delta `dt` (seconds) and time constant `tau`.
pub(super) fn ease_factor(dt: f64, tau: f64) -> f64 {
    1.0 - (-dt / tau).exp()
}

/// One ease step toward `target` from `current` with factor `k`.
pub(super) fn ease_step(current: f64, target: f64, k: f64) -> f64 {
    current + (target - current) * k
}

/// Whether an animating value has reached its target closely enough to stop.
pub(super) fn is_settled(current: f64, target: f64) -> bool {
    (target - current).abs() <= SETTLE_EPS
}

/// The active index after the tab at `removed` is deleted, given the prior
/// active index. `None` when the active tab itself was removed (the caller then
/// picks a neighbour via [`neighbor_after_remove`]); an index above `removed`
/// shifts down by one to track its entry; below is unchanged.
pub(super) fn active_after_remove(active: Option<usize>, removed: usize) -> Option<usize> {
    match active {
        Some(a) if a == removed => None,
        Some(a) if a > removed => Some(a - 1),
        other => other,
    }
}

/// The neighbour to switch to after the active tab at `removed` is deleted,
/// leaving `remaining` tabs: the tab that slid into the closed slot, or the new
/// last tab if the closed one was last. `None` when no tabs remain — the same
/// neighbour `GtkNotebook` auto-advanced to.
pub(super) fn neighbor_after_remove(removed: usize, remaining: usize) -> Option<usize> {
    (remaining > 0).then(|| removed.min(remaining - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(start: f64, width: f64) -> Span {
        Span { start, width }
    }

    // ── resolve_gutter ──────────────────────────────────────────────────────

    #[test]
    fn gutter_no_overflow_reserves_nothing() {
        // content fits the width ⇒ tabs get the whole width, no chevrons.
        let g = resolve_gutter(400, 300.0, 20);
        assert_eq!(
            g,
            Gutter {
                reserved: 0,
                viewport_w: 400
            }
        );
    }

    #[test]
    fn gutter_overflow_reserves_both_chevrons() {
        // content wider than width ⇒ reserve a chevron each side.
        let g = resolve_gutter(400, 900.0, 20);
        assert_eq!(
            g,
            Gutter {
                reserved: 20,
                viewport_w: 360
            }
        );
    }

    #[test]
    fn gutter_too_narrow_for_chevrons_skips_them() {
        // width < 2*chevron_w ⇒ can't safely show chevrons even though it
        // overflows; skip for this pass rather than allocate a negative chevron.
        let g = resolve_gutter(30, 900.0, 20);
        assert_eq!(g.reserved, 0);
        assert_eq!(g.viewport_w, 30);
    }

    #[test]
    fn gutter_sub_pixel_overflow_ignored() {
        // full_content_w just barely over width (< 0.5px) is not an overflow.
        let g = resolve_gutter(400, 400.4, 20);
        assert_eq!(g.reserved, 0);
    }

    #[test]
    fn gutter_viewport_never_negative() {
        let g = resolve_gutter(0, 900.0, 20);
        assert_eq!(g.viewport_w, 0);
    }

    // ── scroll_extent / page_steps ──────────────────────────────────────────

    #[test]
    fn scroll_extent_clamps_upper_to_viewport() {
        let (upper, max_off) = scroll_extent(100.0, 400.0);
        assert_eq!(upper, 400.0); // upper never below the viewport
        assert_eq!(max_off, 0.0); // ⇒ can't scroll
    }

    #[test]
    fn scroll_extent_overflow_gives_offset_room() {
        let (upper, max_off) = scroll_extent(900.0, 360.0);
        assert_eq!(upper, 900.0);
        assert_eq!(max_off, 540.0);
    }

    #[test]
    fn page_steps_floor_at_min() {
        let (step, page) = page_steps(0.0);
        assert_eq!(step, MIN_STEP);
        assert_eq!(page, MIN_STEP);
        let (step, page) = page_steps(1000.0);
        assert_eq!(step, 100.0);
        assert_eq!(page, 900.0);
    }

    // ── chevron_visibility ──────────────────────────────────────────────────

    #[test]
    fn chevrons_hidden_when_not_overflowing() {
        let (prev, next) = chevron_visibility(0, 0.0, 0.0);
        assert!(!prev && !next);
    }

    #[test]
    fn chevron_prev_only_at_left_edge() {
        let (prev, next) = chevron_visibility(20, 0.0, 540.0);
        assert!(!prev && next); // at start: no prev, next available
    }

    #[test]
    fn chevron_next_only_at_right_edge() {
        let (prev, next) = chevron_visibility(20, 540.0, 540.0);
        assert!(prev && !next); // fully scrolled: prev available, no next
    }

    #[test]
    fn chevrons_both_in_the_middle() {
        let (prev, next) = chevron_visibility(20, 200.0, 540.0);
        assert!(prev && next);
    }

    // ── hit_index ───────────────────────────────────────────────────────────

    #[test]
    fn hit_index_finds_containing_span() {
        let spans = [span(0.0, 100.0), span(100.0, 80.0), span(180.0, 120.0)];
        assert_eq!(hit_index(0.0, &spans), Some(0));
        assert_eq!(hit_index(99.9, &spans), Some(0));
        assert_eq!(hit_index(100.0, &spans), Some(1)); // half-open: start is inside
        assert_eq!(hit_index(250.0, &spans), Some(2));
    }

    #[test]
    fn hit_index_misses_past_the_end() {
        let spans = [span(0.0, 100.0)];
        assert_eq!(hit_index(100.0, &spans), None);
        assert_eq!(hit_index(-1.0, &spans), None);
    }

    // ── reorder_index ───────────────────────────────────────────────────────

    #[test]
    fn reorder_stays_put_when_hover_over_own_slot() {
        // three equal 100px tabs; drag tab 1, hover its own middle (150).
        let spans = [span(0.0, 100.0), span(100.0, 100.0), span(200.0, 100.0)];
        assert_eq!(reorder_index(150.0, 1, &spans), 1);
    }

    #[test]
    fn reorder_moves_right_past_a_midpoint() {
        let spans = [span(0.0, 100.0), span(100.0, 100.0), span(200.0, 100.0)];
        // drag tab 0, hover past tab 2's midpoint (250) ⇒ index 2.
        assert_eq!(reorder_index(260.0, 0, &spans), 2);
    }

    #[test]
    fn reorder_moves_left_to_front() {
        let spans = [span(0.0, 100.0), span(100.0, 100.0), span(200.0, 100.0)];
        // drag tab 2, hover before tab 0's midpoint (50) ⇒ index 0.
        assert_eq!(reorder_index(10.0, 2, &spans), 0);
    }

    // ── scroll_target ───────────────────────────────────────────────────────

    #[test]
    fn scroll_target_none_when_visible() {
        assert_eq!(scroll_target(50.0, 40.0, 0.0, 200.0), None);
    }

    #[test]
    fn scroll_target_left_when_before_viewport() {
        assert_eq!(scroll_target(20.0, 40.0, 100.0, 200.0), Some(20.0));
    }

    #[test]
    fn scroll_target_right_when_past_viewport() {
        // handle at 300 width 40, viewport 200 scrolled to 0 ⇒ need 140.
        assert_eq!(scroll_target(300.0, 40.0, 0.0, 200.0), Some(140.0));
    }

    #[test]
    fn scroll_target_clamps_non_negative() {
        assert_eq!(scroll_target(-10.0, 40.0, 100.0, 200.0), Some(0.0));
    }

    // ── easing ──────────────────────────────────────────────────────────────

    #[test]
    fn ease_step_moves_fraction_toward_target() {
        assert_eq!(ease_step(0.0, 100.0, 0.5), 50.0);
        assert_eq!(ease_step(100.0, 100.0, 0.5), 100.0);
    }

    #[test]
    fn ease_factor_in_unit_range_and_grows_with_dt() {
        let tau = 0.055;
        let small = ease_factor(1.0 / 60.0, tau);
        let large = ease_factor(0.1, tau);
        assert!(small > 0.0 && small < 1.0);
        assert!(large > small && large < 1.0);
    }

    #[test]
    fn is_settled_within_half_pixel() {
        assert!(is_settled(100.0, 100.3));
        assert!(!is_settled(100.0, 101.0));
    }

    // ── remove bookkeeping ──────────────────────────────────────────────────

    #[test]
    fn active_after_remove_active_itself_clears() {
        assert_eq!(active_after_remove(Some(2), 2), None);
    }

    #[test]
    fn active_after_remove_above_shifts_down() {
        assert_eq!(active_after_remove(Some(3), 1), Some(2));
    }

    #[test]
    fn active_after_remove_below_unchanged() {
        assert_eq!(active_after_remove(Some(1), 3), Some(1));
        assert_eq!(active_after_remove(None, 3), None);
    }

    #[test]
    fn neighbor_after_remove_picks_slid_in_or_last() {
        assert_eq!(neighbor_after_remove(1, 3), Some(1)); // tab that slid in
        assert_eq!(neighbor_after_remove(3, 3), Some(2)); // closed last ⇒ new last
        assert_eq!(neighbor_after_remove(0, 0), None); // none left
    }
}
