//! Pure, GTK-free layout arithmetic for [`super::ScribTableWidget`].
//!
//! Extracted from the widget's `recompute` so the load-bearing decisions —
//! how columns are fitted into a bound width, and how cells are placed into a
//! grid — are **unit-testable without a live display** (POLICY §coverage gate:
//! "extract a pure decision core into a logic module and unit-test it"). The
//! only GTK the widget still owns is the `measure()` calls that produce the
//! per-column min/natural widths and per-row heights fed in here; everything
//! downstream of those measurements is plain integer arithmetic.

/// Minimum width (px) a column is allowed to shrink to when the table is wider
/// than the viewport (cells wrap within it). Below this, content becomes
/// unreadable. Applied to each column's minimum by the caller before
/// [`fit_columns`] (so the fit never allocates a column below it).
pub(crate) const MIN_COL_WIDTH: i32 = 48;

/// One cell's grid position (row-major), parallel to the widget's cell list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CellPos {
    pub(crate) row: usize,
    pub(crate) col: usize,
}

/// A placed cell rectangle in widget-local coordinates (converted to a
/// `gdk::Rectangle` by the caller, keeping this module free of any GTK type).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CellRect {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) w: i32,
    pub(crate) h: i32,
}

/// The full placement result: the widget's total (width, height) and one rect
/// per input cell (parallel to the `cells` slice given to [`place_cells`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Placement {
    pub(crate) total: (i32, i32),
    pub(crate) rects: Vec<CellRect>,
}

/// One column's two measurements, in pixels.
///
/// **Named so the pair cannot be transposed**, and named identically to
/// `export::pdftable::ColumnWant`, which is this function's transcription into points.
/// The two took their equivalent pairs in OPPOSITE orders — this one min-then-natural,
/// the export's natural-then-minimum — so the cross-check test binding them had to
/// hand-swap its arrays to be true. A transposed call neither panics nor warns: every
/// column's floor becomes its want and vice versa, and the table merely lays out badly.
///
/// It also retires the `debug_assert_eq!` this function used to carry for the two
/// slices' lengths: one slice of pairs cannot disagree with itself about how many
/// columns there are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ColumnWant {
    /// Max-content: the column's widest cell, unconstrained.
    pub(crate) natural: i32,
    /// Min-content: below this a cell overflows its column.
    pub(crate) minimum: i32,
}

/// Fit the columns into `bound_w`.
///
/// **What is actually guaranteed, which is not what this said.** It claimed
/// `sum(col_w) <= bound_w` AND `col_w[c] >= minimum[c]` unconditionally — and its own
/// middle branch violates the first: when not even the minimums fit, it RETURNS the
/// minimums, whose sum exceeds the bound by construction. That is the correct behaviour
/// and the only honest one (the table is `sum(min)` wide and the pane h-scrolls), but a
/// caller reading the guarantee and sizing something against it would have been wrong in
/// exactly the case that matters.
///
/// So, restated as the code behaves:
///   - `col_w[c] >= minimum[c]` — ALWAYS. No cell ever overflows its column.
///   - `sum(col_w) <= bound_w` — WHENEVER THE MINIMUMS FIT. Where they do not, the result
///     is `sum(minimum)` and the caller must expect to scroll; `widgets::table::mod`
///     does, which is why the outer Automatic h-bar and the split-pane blank it can
///     re-arm (GTK4Rs/AP-23) are not triggered by it.
///
/// Three cases:
///   - naturals fit → give each its natural plus an even share of the slack;
///   - even the minimums don't fit (very narrow pane) → use the minimums (the
///     table is `sum(min)` wide and h-scrolls — the only honest option);
///   - in between → water-fill: process columns in ascending natural-width
///     order, giving each the smaller of (its want) and (its equal share of
///     what's left). A column that saturates early (a narrow identifier) gets
///     its full natural and yields the surplus to wider columns, instead of a
///     proportional-to-want scheme that would give nearly all space to the
///     widest column.
///
/// Each column's `minimum` is assumed already clamped to at least [`MIN_COL_WIDTH`] by
/// the caller (the measure loop). `natural` is normalised to `>= minimum` here, so
/// callers need not pre-normalise it.
pub(crate) fn fit_columns(wants: &[ColumnWant], bound_w: i32) -> Vec<i32> {
    let ncols = wants.len();
    if ncols == 0 {
        return Vec::new();
    }

    let col_min: Vec<i32> = wants.iter().map(|w| w.minimum).collect();
    // Natural is never less than minimum.
    let col_nat: Vec<i32> = wants.iter().map(|w| w.natural.max(w.minimum)).collect();

    let sum_min: i32 = col_min.iter().sum();
    let sum_nat: i32 = col_nat.iter().sum();

    if sum_nat <= bound_w {
        // Naturals fit — even share of the slack to every column.
        let per = (bound_w - sum_nat) / ncols as i32;
        col_nat.iter().map(|&w| w + per).collect()
    } else if sum_min >= bound_w {
        // Even the minimums overflow — use them; the table h-scrolls.
        col_min.clone()
    } else {
        // Water-fill: sort by natural width ascending so narrower columns are
        // offered their fair share first. A column whose want <= fair share
        // gets its natural; the surplus pools for the remaining wider columns.
        let mut order: Vec<usize> = (0..ncols).collect();
        order.sort_by_key(|&c| col_nat[c]);

        let mut col_w = col_min.clone();
        let mut pool = (bound_w - sum_min) as i64;

        for (i, &c) in order.iter().enumerate() {
            let remaining = (ncols - i) as i64;
            let fair = pool / remaining;
            let want = (col_nat[c] - col_min[c]) as i64;
            let give = want.min(fair);
            col_w[c] += give as i32;
            pool -= give;
        }
        // Residual pixels from integer division go to the widest column.
        // `order` is non-empty here (ncols >= 1), but avoid unwrap()ing on that
        // invariant — fall through to a no-op if it is ever violated.
        if pool > 0 {
            if let Some(&widest) = order.last() {
                col_w[widest] += pool as i32;
            }
        }
        col_w
    }
}

/// Given per-column widths, per-row heights, and each cell's grid position,
/// compute the prefix-sum column x-origins and row y-origins, each cell's full
/// column-width × row-height rectangle (so each cell's border box fills its grid
/// slot; text top-aligns within), and the widget's total size. Returns an empty
/// [`Placement`] when there are no columns or rows.
pub(crate) fn place_cells(col_w: &[i32], row_h: &[i32], cells: &[CellPos]) -> Placement {
    let ncols = col_w.len();
    let nrows = row_h.len();
    if ncols == 0 || nrows == 0 {
        return Placement::default();
    }

    // Prefix sums → x of each column, y of each row.
    let mut col_x = vec![0i32; ncols];
    for c in 1..ncols {
        col_x[c] = col_x[c - 1] + col_w[c - 1];
    }
    let mut row_y = vec![0i32; nrows];
    for r in 1..nrows {
        row_y[r] = row_y[r - 1] + row_h[r - 1];
    }

    let rects = cells
        .iter()
        .map(|cell| CellRect {
            x: col_x[cell.col],
            y: row_y[cell.row],
            w: col_w[cell.col],
            h: row_h[cell.row],
        })
        .collect();

    let total = (col_w.iter().sum(), row_h.iter().sum());
    Placement { total, rects }
}

#[cfg(test)]
mod tests {
    /// Pair minimums with naturals for a test. Takes them in this function's own
    /// documented order (minimum first, as the measure loop produces them) and builds
    /// the pairs, so a fixture cannot express the transposition the type prevents.
    fn wants(col_min: &[i32], col_nat: &[i32]) -> Vec<ColumnWant> {
        assert_eq!(col_min.len(), col_nat.len(), "fixture length mismatch");
        col_min
            .iter()
            .zip(col_nat)
            .map(|(&minimum, &natural)| ColumnWant { natural, minimum })
            .collect()
    }

    use super::*;

    /// **QA M08** — the contract the doc comment states, including its exception.
    ///
    /// The comment used to GUARANTEE `sum(col_w) <= bound_w` unconditionally, which its
    /// own middle branch violates: when not even the minimums fit it returns the minimums,
    /// whose sum exceeds the bound by construction. Both halves are pinned here so the
    /// restated contract cannot drift back into the stronger, false one.
    #[test]
    fn the_stated_contract_holds_including_the_case_that_breaks_the_bound() {
        // Minimums fit: both clauses hold.
        let w = fit_columns(&wants(&[40, 30, 20], &[300, 200, 120]), 400);
        assert!(w.iter().sum::<i32>() <= 400, "must fit the bound: {w:?}");
        for (got, want) in w.iter().zip([40, 30, 20]) {
            assert!(*got >= want, "no cell may overflow its column: {w:?}");
        }

        // Minimums do NOT fit: the floor clause still holds, the bound clause does not,
        // and that is the documented exception rather than a defect.
        let narrow = fit_columns(&wants(&[200, 150, 120], &[300, 200, 120]), 100);
        for (got, want) in narrow.iter().zip([200, 150, 120]) {
            assert_eq!(*got, want, "the minimums are returned verbatim: {narrow:?}");
        }
        assert!(
            narrow.iter().sum::<i32>() > 100,
            "and they exceed the bound, which is why the pane h-scrolls: {narrow:?}"
        );
    }

    #[test]
    fn fit_columns_empty_is_empty() {
        assert!(fit_columns(&wants(&[], &[]), 500).is_empty());
    }

    #[test]
    fn fit_columns_naturals_fit_shares_slack_evenly() {
        // sum_nat = 300 <= bound 360 ⇒ 60 slack / 3 cols = 20 each.
        let col_w = fit_columns(&wants(&[50, 50, 50], &[100, 100, 100]), 360);
        assert_eq!(col_w, vec![120, 120, 120]);
        assert!(col_w.iter().sum::<i32>() <= 360);
    }

    #[test]
    fn fit_columns_naturals_fit_exactly_no_slack() {
        let col_w = fit_columns(&wants(&[50, 50], &[100, 100]), 200);
        assert_eq!(col_w, vec![100, 100]);
    }

    #[test]
    fn fit_columns_minimums_overflow_returns_minimums() {
        // sum_min = 300 >= bound 200 ⇒ minimums, table h-scrolls.
        let col_w = fit_columns(&wants(&[150, 150], &[400, 400]), 200);
        assert_eq!(col_w, vec![150, 150]);
    }

    #[test]
    fn fit_columns_waterfill_saturates_narrow_yields_to_wide() {
        // sum_min=100, sum_nat=400, bound=250 (between). Narrow col (nat 120,
        // min 50, want 70) is offered fair 150/2=75 first ⇒ takes its full 70;
        // the 5 surplus pools to the wide col (nat 280, min 50), which then
        // gets the rest. Total must equal bound and honour minimums.
        let col_w = fit_columns(&wants(&[50, 50], &[280, 120]), 250);
        assert_eq!(col_w.iter().sum::<i32>(), 250);
        assert!(col_w[0] >= 50 && col_w[1] >= 50);
        // Narrow column (index 1) got its full natural, not an even split.
        assert_eq!(col_w[1], 120);
        assert_eq!(col_w[0], 130);
    }

    #[test]
    fn fit_columns_waterfill_residual_pixel_to_widest() {
        // Force an odd pool so integer division leaves a residual pixel that
        // must land on the widest (last-in-order) column; total stays == bound.
        let col_w = fit_columns(&wants(&[10, 10, 10], &[200, 30, 40]), 101);
        assert_eq!(col_w.iter().sum::<i32>(), 101);
        // Widest column is index 0 (nat 200) — it absorbs the residual.
        assert!(col_w[0] >= col_w[1] && col_w[0] >= col_w[2]);
    }

    #[test]
    fn fit_columns_never_below_minimum() {
        // A degenerate natural below the minimum must still allocate >= min.
        let col_w = fit_columns(&wants(&[80, 80], &[10, 10]), 400);
        assert!(col_w.iter().all(|&w| w >= 80));
    }

    #[test]
    fn fit_columns_respects_its_stated_contract_across_a_sweep_of_shapes() {
        // Mirrors `pdftable`'s
        // `every_width_respects_its_invariants_across_a_sweep_of_shapes`: a sweep of
        // column counts and bounds spanning all three regimes (fits-outright, water-fill,
        // floors-don't-fit), checking the two-clause contract this function's own doc
        // comment states — `col_w[c] >= minimum[c]` always, `sum(col_w) <= bound_w`
        // whenever the minimums fit — as a property rather than one example per regime.
        for ncols in 1..8_usize {
            let col_min: Vec<i32> = (0..ncols).map(|i| 10 + i as i32 * 5).collect();
            let col_nat: Vec<i32> = col_min.iter().map(|&m| m * 4).collect();
            let sum_min: i32 = col_min.iter().sum();
            let sum_nat: i32 = col_nat.iter().sum();
            for bound in [
                (sum_min - 5).max(0),
                sum_min,
                (sum_min + sum_nat) / 2,
                sum_nat,
                sum_nat + 50,
            ] {
                let col_w = fit_columns(&wants(&col_min, &col_nat), bound);
                assert_eq!(
                    col_w.len(),
                    ncols,
                    "column count changed at ncols={ncols} bound={bound}"
                );
                for (i, &w) in col_w.iter().enumerate() {
                    assert!(
                        w >= col_min[i],
                        "column {i} fell below its minimum at ncols={ncols} bound={bound}: \
                         {col_w:?}"
                    );
                }
                let total: i32 = col_w.iter().sum();
                if sum_min < bound {
                    assert!(
                        total <= bound,
                        "sum exceeded the bound though the minimums fit at ncols={ncols} \
                         bound={bound}: {col_w:?}"
                    );
                } else {
                    assert_eq!(
                        total, sum_min,
                        "minimums must be returned verbatim when they do not fit at \
                         ncols={ncols} bound={bound}: {col_w:?}"
                    );
                }

                // Determinism: the same inputs must produce the same outputs.
                let col_w_again = fit_columns(&wants(&col_min, &col_nat), bound);
                assert_eq!(col_w, col_w_again, "fit_columns is non-deterministic");
            }
        }
    }

    #[test]
    fn place_cells_empty_when_no_columns_or_rows() {
        assert_eq!(place_cells(&[], &[10], &[]), Placement::default());
        assert_eq!(place_cells(&[10], &[], &[]), Placement::default());
    }

    #[test]
    fn place_cells_prefix_sums_and_total() {
        let cells = [
            CellPos { row: 0, col: 0 },
            CellPos { row: 0, col: 1 },
            CellPos { row: 1, col: 0 },
            CellPos { row: 1, col: 1 },
        ];
        let placed = place_cells(&[100, 60], &[20, 30], &cells);
        assert_eq!(placed.total, (160, 50));
        assert_eq!(
            placed.rects[0],
            CellRect {
                x: 0,
                y: 0,
                w: 100,
                h: 20
            }
        );
        assert_eq!(
            placed.rects[1],
            CellRect {
                x: 100,
                y: 0,
                w: 60,
                h: 20
            }
        );
        assert_eq!(
            placed.rects[2],
            CellRect {
                x: 0,
                y: 20,
                w: 100,
                h: 30
            }
        );
        assert_eq!(
            placed.rects[3],
            CellRect {
                x: 100,
                y: 20,
                w: 60,
                h: 30
            }
        );
    }
}
