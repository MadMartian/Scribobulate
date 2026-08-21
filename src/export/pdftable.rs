//! Column geometry for a table on a printed page — display-free.
//!
//! # Why this is its own file
//!
//! Deciding how wide each column is needs a list of natural widths, a page width and a
//! rule. It does not need Pango, cairo or a display — so it sits here, inside the
//! coverage gate, on the same seam [`super::paginate`] sits on: `pdf.rs` measures and
//! draws, this file decides. The decision is settled by unit test rather than by a
//! human holding a ruler against a viewer.
//!
//! # The rule
//!
//! A table is laid out on a **measured column grid**, never on tab stops. Tab stops
//! were the original implementation and they cannot align columns even in principle:
//! a tab advances to the next stop in a fixed ladder, so a cell one character too wide
//! pushes its neighbour a whole stop to the right and the column zig-zags down the
//! page. (The input side of this project already learned that hard tabs and tables mix
//! badly — ScrAP-75. This is the same trap facing the other way.)
//!
//! Three regimes, in order, and the order is the point:
//!
//! 1. **It fits** — every column gets its natural width. A narrow table stays narrow
//!    rather than being stretched to the margin, which is what the preview does.
//! 2. **It does not fit** — the columns share what there is by water-fill (see
//!    [`distribute`]) and their text **wraps inside the column**. The column
//!    *structure* is preserved, which is what TDD 25.17's "never reflowed into a
//!    different table" protects; no column is dropped, merged, or degraded into a list.
//! 3. **Even the minimum does not fit** — a uniform scale is applied to the whole
//!    table, TDD 25.17's scale-to-fit. This is the last resort and not the first,
//!    because scaling is the regime that makes text unreadable: reaching for it while
//!    honest wrapping was still available shrinks a table nobody needed shrunk.
//!
//! # Its relationship to `widgets::table::layout::fit_columns`
//!
//! The preview solves the same-shaped problem — minimums, naturals, a bound — and
//! **this file uses the preview's rule**: water-fill, described at [`distribute`].
//! Document Rendering CAM row 17 asks an export to show what the preview showed, and
//! column widths are part of that; two different sharing rules would drift visibly on
//! the same document with nothing keeping them honest.
//!
//! Exactly **one** thing differs, and only because the surfaces genuinely differ:
//! what happens when not even the minimum widths fit. A pane hands the reader a
//! horizontal scrollbar and lets the table stay its natural size; a page has no such
//! move, so where the preview stops at `sum(min)` and scrolls, regime 3 here scales the
//! whole table down. That is the whole of the divergence, and it is confined to the
//! regime where the preview's answer is unavailable rather than merely different.
//!
//! **This was got wrong once, and the shape of the error is worth keeping.** The first
//! version of this file shared space proportionally to what each column wanted, and
//! justified the divergence with the scrolling argument above. The argument is sound
//! about the overflow endgame and says nothing whatever about how space is shared when
//! it fits — so a real difference in one regime was used to wave through an unrelated
//! difference in the others. A justification that is true is not thereby a
//! justification for the thing in front of you; check that it reaches.
//!
//! The rule now lives in two places, which is a real cost and is **not** a licence to
//! add a third. It is duplicated rather than shared because the preview's copy works in
//! `i32` device pixels inside a gated widget module and this one in `f64` points, and
//! refactoring the live preview's layout was not a trade worth making inside an export
//! bugfix. If a third consumer ever appears, that is the moment to lift the rule into
//! one module all of them call.

/// The narrowest any column may ever be, in points.
///
/// A backstop, not the working floor: the real floor is each column's **min-content**
/// width — its longest unbreakable word — which the caller measures. This exists only
/// so a column of entirely empty cells still has somewhere to draw its borders.
pub(crate) const MIN_COLUMN_PT: f64 = 8.0;

/// The chrome a table draws around and between its cells, in points.
///
/// Supplied by the caller from the active theme — there is no literal here, for the
/// same reason there is no hex value in `pdf.rs` (THEMING.md; Document Rendering CAM
/// row 12: one theme key feeds every application path).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Chrome {
    /// Horizontal padding inside each cell, applied on both sides.
    pub(crate) padding_h: f64,
    /// Vertical padding inside each cell, applied top and bottom.
    pub(crate) padding_v: f64,
    /// Width of one cell border line.
    pub(crate) border: f64,
}

/// Where one column sits and how much text it may hold.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Column {
    /// Left edge of this column's **border box**, relative to the table's left edge.
    pub(crate) x: f64,
    /// Width of the border box — text width plus this table's horizontal padding.
    pub(crate) box_width: f64,
    /// Width available to the cell's own text, padding already removed. This is what
    /// a Pango layout is given, so it is what decides where the text wraps.
    pub(crate) text_width: f64,
}

/// A whole table's horizontal geometry.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Grid {
    pub(crate) columns: Vec<Column>,
    /// Total width of the table's border box, chrome included, **before** `scale`.
    pub(crate) total: f64,
    /// Uniform scale to draw the table at, in `(0, 1]`. Anything below 1.0 is
    /// regime 3 — see the module docs.
    pub(crate) scale: f64,
}

impl Grid {
    /// The width this table actually occupies on the page, scale applied.
    ///
    /// Test-only: the drawing pass never needs it, because it applies `scale` as a
    /// transform and then works in unscaled coordinates. It exists so "scaled to fit"
    /// can be asserted as a number rather than inferred from the scale factor.
    #[cfg(test)]
    pub(crate) fn drawn_width(&self) -> f64 {
        self.total * self.scale
    }
}

/// Decide column widths for one table.
///
/// `natural` is each column's widest cell measured **unconstrained** (its max-content
/// width) and `minimum` the widest word that column cannot break (its min-content
/// width); `available` is the printable width the table has to live in. All in points.
///
/// The two measurements are what stop a squeeze from hyphen-breaking a short label: a
/// column may be offered less than it wants, but never less than one whole word of what
/// it holds, so the squeeze lands where there is prose to absorb it.
pub(crate) fn fit(natural: &[f64], minimum: &[f64], available: f64, chrome: &Chrome) -> Grid {
    let count = natural.len();
    if count == 0 {
        return Grid {
            columns: Vec::new(),
            total: 0.0,
            scale: 1.0,
        };
    }

    // Chrome is charged per column: one border down each column's left edge, one more
    // closing the table, and padding on both sides of every cell.
    let per_column_chrome = chrome.border + chrome.padding_h * 2.0;
    let total_chrome = per_column_chrome * count as f64 + chrome.border;
    let text_available = available - total_chrome;

    // Each column's own floor: its longest unbreakable word, never below the backstop.
    let floors: Vec<f64> = (0..count)
        .map(|i| {
            minimum
                .get(i)
                .copied()
                .unwrap_or(MIN_COLUMN_PT)
                .max(MIN_COLUMN_PT)
                // A floor may not exceed what the column actually wants, or a narrow
                // column would be padded out past its own content.
                .min(natural[i].max(MIN_COLUMN_PT))
        })
        .collect();

    // A page too narrow to hold the chrome alone: nothing sane is left to divide, so
    // fall through to the floors and let the scale regime rescue it.
    let text_widths = if text_available <= 0.0 {
        floors.clone()
    } else {
        distribute(natural, &floors, text_available)
    };

    // Regime 3: the floor still overflows, so the whole table is scaled down. Computed
    // against the width the table actually wants, so the result lands exactly on the
    // available width rather than approaching it.
    let wanted: f64 = text_widths.iter().sum::<f64>() + total_chrome;
    let scale = if wanted > available && wanted > 0.0 {
        available / wanted
    } else {
        1.0
    };

    let mut columns = Vec::with_capacity(count);
    let mut x = 0.0;
    for text_width in &text_widths {
        let box_width = text_width + per_column_chrome;
        columns.push(Column {
            x,
            box_width,
            text_width: *text_width,
        });
        x += box_width;
    }

    Grid {
        columns,
        total: wanted,
        scale,
    }
}

/// Share `available` points of text width between columns wanting `natural`, none
/// falling below its own entry in `floor`.
///
/// **Water-fill, and deliberately the same rule the preview uses**
/// (`widgets::table::layout::fit_columns`): offer every column an equal share of what
/// is left, in ascending order of what it wants, so a column that wants less than its
/// share takes only what it wants and yields the surplus to the wider columns behind
/// it. A narrow identifier column therefore keeps its whole natural width while the
/// prose column beside it absorbs the squeeze.
///
/// The rejected alternative is proportional-to-want, and the preview's own comment
/// rejects it in those words: sharing in proportion to what each column *wants* gives
/// nearly all the space to the widest column and starves the narrow ones — which shows
/// up as a one-word heading hyphen-broken down the page beside a sentence with room to
/// spare. This file shipped that scheme once; the divergence was not defensible,
/// because the reason the two surfaces differ at all (a pane can scroll, a page cannot)
/// bears **only** on what happens when nothing fits, and says nothing about how space
/// is shared when it does.
fn distribute(natural: &[f64], floor: &[f64], available: f64) -> Vec<f64> {
    let count = natural.len();
    // A natural width is never less than the column's own floor.
    let natural: Vec<f64> = (0..count).map(|i| natural[i].max(floor[i])).collect();

    let sum_natural: f64 = natural.iter().sum();
    if sum_natural <= available {
        // Everything fits. Unlike the preview, the slack is NOT shared out: a pane's
        // table fills the pane, but on paper a three-column table stretched to the
        // margin looks like a mistake, and the HTML sink's `<table>` shrinks to fit
        // too — so the two ARTEFACTS agree here, which is the agreement that matters.
        return natural;
    }

    let sum_floor: f64 = floor.iter().sum();
    if sum_floor >= available {
        // Not even the floors fit; the scale regime takes it from here.
        return floor.to_vec();
    }

    let mut order: Vec<usize> = (0..count).collect();
    order.sort_by(|a, b| natural[*a].total_cmp(&natural[*b]));

    let mut widths = floor.to_vec();
    let mut pool = available - sum_floor;
    for (offered, column) in order.iter().enumerate() {
        let remaining = (count - offered) as f64;
        let fair = pool / remaining;
        let give = (natural[*column] - floor[*column]).min(fair).max(0.0);
        widths[*column] += give;
        pool -= give;
    }
    widths
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Chrome with round numbers, so an assertion reads as arithmetic rather than as a
    /// restatement of whatever the theme happened to resolve to.
    fn chrome() -> Chrome {
        Chrome {
            padding_h: 4.0,
            padding_v: 2.0,
            border: 1.0,
        }
    }

    /// No chrome at all, for the tests that are about the sharing arithmetic itself.
    fn bare() -> Chrome {
        Chrome {
            padding_h: 0.0,
            padding_v: 0.0,
            border: 0.0,
        }
    }

    fn widths(grid: &Grid) -> Vec<f64> {
        grid.columns.iter().map(|c| c.text_width).collect()
    }

    #[test]
    fn a_table_that_fits_keeps_every_columns_natural_width() {
        let grid = fit(&[100.0, 50.0, 60.0], &[20.0, 20.0, 20.0], 500.0, &chrome());
        assert_eq!(widths(&grid), vec![100.0, 50.0, 60.0]);
        assert_eq!(grid.scale, 1.0, "a table that fits is never scaled");
    }

    #[test]
    fn a_narrow_table_is_not_stretched_to_the_margin() {
        // Regime 1 leaves slack rather than spreading three short columns across a
        // whole page, which is what the preview does and what a reader expects.
        let grid = fit(&[20.0, 20.0, 20.0], &[10.0, 10.0, 10.0], 500.0, &chrome());
        assert!(
            grid.total < 200.0,
            "a narrow table grew to {}pt of a 500pt page",
            grid.total
        );
    }

    #[test]
    fn columns_are_laid_end_to_end_with_no_gap_and_no_overlap() {
        let grid = fit(&[100.0, 50.0, 60.0], &[20.0, 20.0, 20.0], 500.0, &chrome());
        let mut expected_x = 0.0;
        for column in &grid.columns {
            assert_eq!(column.x, expected_x, "column {column:?} is out of place");
            expected_x += column.box_width;
        }
    }

    #[test]
    fn an_over_wide_table_water_fills_rather_than_sharing_in_proportion() {
        // Every column wants more than an equal share, so each gets exactly that —
        // NOT [100, 50, 50], which is what proportional-to-want would hand back.
        let grid = fit(&[200.0, 100.0, 100.0], &[10.0, 10.0, 10.0], 200.0, &bare());
        for w in widths(&grid) {
            assert!((w - 200.0 / 3.0).abs() < 0.01, "{:?}", widths(&grid));
        }
        assert_eq!(grid.scale, 1.0, "wrapping was available; nothing to scale");
    }

    #[test]
    fn a_narrow_column_keeps_its_whole_natural_width_when_a_wide_one_must_shrink() {
        // The property proportional-to-want gets wrong, and the reason this file uses
        // the preview's rule: the two short columns want less than their equal share,
        // so they are satisfied outright and the prose column absorbs all the squeeze.
        let grid = fit(&[600.0, 60.0, 60.0], &[40.0, 55.0, 55.0], 300.0, &bare());
        let w = widths(&grid);
        assert!(
            (w[1] - 60.0).abs() < 0.01,
            "short column was clipped: {w:?}"
        );
        assert!(
            (w[2] - 60.0).abs() < 0.01,
            "short column was clipped: {w:?}"
        );
        assert!(
            (w[0] - 180.0).abs() < 0.01,
            "prose column should absorb it: {w:?}"
        );
    }

    #[test]
    fn the_shared_rule_agrees_with_the_previews_own_fit_columns() {
        // CAM row 17: the export's columns must be the preview's columns. Same inputs
        // through both implementations (i32 px vs f64 pt) must land on the same shape.
        let min = [40_i32, 55, 55];
        let nat = [600_i32, 60, 60];
        let preview = crate::widgets::table::layout::fit_columns(&min, &nat, 300);
        let ours = widths(&fit(
            &[600.0, 60.0, 60.0],
            &[40.0, 55.0, 55.0],
            300.0,
            &bare(),
        ));
        for (theirs, mine) in preview.iter().zip(&ours) {
            assert!(
                (f64::from(*theirs) - mine).abs() <= 1.0,
                "preview {preview:?} vs export {ours:?}"
            );
        }
    }

    #[test]
    fn the_column_count_survives_any_squeeze() {
        // TDD 25.17's "never reflowed into a different table": the structure is the
        // invariant, at every width including absurd ones.
        for available in [1.0, 10.0, 80.0, 200.0, 1000.0] {
            let grid = fit(
                &[300.0, 200.0, 120.0, 90.0],
                &[40.0, 30.0, 25.0, 20.0],
                available,
                &chrome(),
            );
            assert_eq!(
                grid.columns.len(),
                4,
                "column count changed at {available}pt available"
            );
        }
    }

    #[test]
    fn a_column_is_never_squeezed_below_its_longest_word() {
        // The regression this file's second draft exists for: proportional sharing
        // alone gave the prose column the page and hyphen-broke the one-word column
        // beside it. The floor is per column and measured, not a global constant.
        let grid = fit(&[600.0, 60.0, 60.0], &[40.0, 55.0, 55.0], 300.0, &bare());
        let w = widths(&grid);
        assert!(
            w[1] >= 55.0 && w[2] >= 55.0,
            "a column was squeezed under its longest word: {w:?}"
        );
        assert!(
            w[0] > w[1],
            "the prose column should still be the widest: {w:?}"
        );
    }

    #[test]
    fn a_floor_never_exceeds_what_the_column_actually_wants() {
        // A column whose whole content is one short word must not be padded out to a
        // floor larger than its own natural width.
        let grid = fit(&[12.0, 400.0], &[12.0, 40.0], 200.0, &bare());
        let w = widths(&grid);
        assert!(w[0] <= 12.0 + f64::EPSILON, "narrow column inflated: {w:?}");
    }

    #[test]
    fn scale_to_fit_is_the_last_resort_and_lands_on_the_page() {
        // Six columns whose longest words alone overflow 120pt: wrapping cannot save
        // it, so regime 3 fires and the drawn width is exactly the page —
        // TDD 25.17: scaled, never clipped.
        let grid = fit(&[200.0; 6], &[60.0; 6], 120.0, &chrome());
        assert!(grid.scale < 1.0, "expected a scale, got {}", grid.scale);
        assert!(
            (grid.drawn_width() - 120.0).abs() < 0.01,
            "drawn width {} missed the 120pt page",
            grid.drawn_width()
        );
    }

    #[test]
    fn wrapping_is_preferred_to_scaling_whenever_it_is_available() {
        // The ordering that keeps type readable: a table that CAN wrap into the page
        // is never shrunk, however far it has to wrap.
        let grid = fit(&[900.0, 900.0], &[30.0, 30.0], 300.0, &bare());
        assert_eq!(grid.scale, 1.0, "scaled when wrapping would have done");
    }

    #[test]
    fn a_scale_is_never_an_upscale() {
        // A table with room to spare is left alone, not blown up to fill the page.
        let grid = fit(&[10.0, 10.0], &[10.0, 10.0], 600.0, &chrome());
        assert_eq!(grid.scale, 1.0);
    }

    #[test]
    fn a_table_with_no_columns_is_geometry_free_rather_than_a_panic() {
        let grid = fit(&[], &[], 500.0, &chrome());
        assert!(grid.columns.is_empty());
        assert_eq!(grid.total, 0.0);
        assert_eq!(grid.scale, 1.0);
    }

    #[test]
    fn a_page_narrower_than_the_chrome_still_produces_a_drawable_grid() {
        // Degenerate, but reachable through a hostile page setup: every width must
        // stay finite and positive or the drawing pass emits nothing at all.
        let grid = fit(&[100.0, 100.0], &[20.0, 20.0], 3.0, &chrome());
        assert!(grid.scale > 0.0 && grid.scale <= 1.0);
        assert!(grid.columns.iter().all(|c| c.text_width > 0.0));
        assert!(grid.drawn_width() <= 3.01);
    }

    #[test]
    fn a_column_of_empty_cells_still_has_a_drawable_width() {
        let grid = fit(&[0.0, 100.0], &[0.0, 20.0], 200.0, &bare());
        assert!(
            widths(&grid)[0] >= MIN_COLUMN_PT,
            "an empty column collapsed to nothing"
        );
    }

    #[test]
    fn every_width_stays_finite_across_a_sweep_of_shapes() {
        // A property sweep rather than another example: the arithmetic has three
        // regimes and a pinning loop, and a NaN anywhere in it draws nothing at all.
        for columns in 1..8_usize {
            for available in [0.5, 5.0, 47.0, 300.0, 5000.0] {
                let natural: Vec<f64> = (0..columns).map(|i| 10.0 + i as f64 * 90.0).collect();
                let minimum: Vec<f64> = natural.iter().map(|n| n / 3.0).collect();
                let grid = fit(&natural, &minimum, available, &chrome());
                assert_eq!(grid.columns.len(), columns);
                assert!(
                    grid.scale.is_finite() && grid.scale > 0.0 && grid.scale <= 1.0,
                    "scale {} at {columns} columns / {available}pt",
                    grid.scale
                );
                for column in &grid.columns {
                    assert!(
                        column.text_width.is_finite() && column.text_width > 0.0,
                        "width {} at {columns} columns / {available}pt",
                        column.text_width
                    );
                    assert!(column.x.is_finite() && column.x >= 0.0);
                }
            }
        }
    }
}
