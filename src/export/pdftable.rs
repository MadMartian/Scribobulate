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
//! **THREE things differ, not one.** This paragraph said "exactly one" and the file
//! contradicted it 180 lines below, at [`distribute`]. A claim of total agreement is worth
//! nothing if a reader has to audit the code to find out it is false, so they are
//! enumerated here — and each is confined to a place where the surfaces genuinely differ
//! rather than being an accident:
//!
//! 1. **Regime 3, when not even the minimum widths fit.** A pane hands the reader a
//!    horizontal scrollbar and lets the table stay its natural size; a page has no such
//!    move, so where the preview stops at `sum(min)` and scrolls, this file scales the
//!    whole table down. The preview's answer is unavailable rather than merely different.
//! 2. **Regime 1, when everything fits.** The preview shares the slack out evenly and
//!    fills the pane; this file returns the natural widths untouched. On paper a
//!    three-column table stretched to the margin reads as a mistake, and the HTML sink's
//!    `<table>` shrinks to fit too — so the two ARTEFACTS agree here even though the two
//!    IMPLEMENTATIONS do not, which is the agreement that matters. Stated at
//!    [`distribute`] as well, where the code is.
//! 3. **Integer residue.** The preview works in whole pixels and hands the remainder of
//!    its division to the widest column; this file works in points and has no remainder
//!    to place. A units difference, not a policy one.
//!
//! What IS shared is the rule the CAM row is about: the water-fill order in regime 2,
//! where space is scarce but sufficient. That is the one that would drift visibly on the
//! same document. `the_declared_divergences_hold_in_regimes_1_and_3` pins where the two
//! implementations must NOT agree; `the_shared_rule_holds_across_a_sweep_of_overflow_shapes`
//! pins regime 2, and it is a SWEEP over many shapes and bounds rather than the single
//! example it began as — one point could not tell "the rule holds" from "these two inputs
//! happen to coincide".
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
//!
//! **A shared primitive for the water-fill loop itself was assessed and rejected
//! (finding M34).** The two loops look identical at a glance — sort ascending by natural
//! width, offer each column the smaller of its own want and an equal share of what is
//! left — but they are not the same arithmetic. This file's `distribute` computes each
//! `fair` share as an exact `f64` division and lets the fraction ride in the result;
//! `fit_columns` computes `fair` by **integer** division at every step, truncating on
//! each of the `count` steps rather than once at the end. Routing `fit_columns` through a
//! shared `f64` primitive (convert in, convert out) does not just change how the shared
//! width is *rounded*, it changes **which column absorbs the truncation**: fed three tied
//! columns (`want = 4` each, `pool = 11`, `remaining = 3`), `fit_columns`'s own per-step
//! truncation gives `[+3, +4, +4]`, while a float-exact-then-floor-with-residual
//! reconstruction of the identical inputs gives `[+3, +3, +5]` — a different column gets
//! the extra pixel, not merely a different-looking remainder. `fit_columns`'s own tests
//! pin specific per-column widths (`fit_columns_waterfill_residual_pixel_to_widest`), so
//! this is not a rounding nicety to smooth over; it is exactly the case POLICY's "a new
//! path is justified only when reuse would force the existing one to serve two masters"
//! describes. The widget's per-step integer truncation and this file's one-shot continuous
//! division are two different rules that agree closely, not one rule in two units, and
//! forcing them through one primitive would make `fit_columns` serve a master (float
//! division) its own pinned tests say it does not have. The minimum was taken instead:
//! the cross-check below is now a sweep over many shapes and bounds, so the agreement the
//! two rules *do* have is demonstrated as a property rather than asserted from one input.

/// The narrowest any column may ever be, in points.
///
/// A backstop, not the working floor: the real floor is each column's **min-content**
/// width — its longest unbreakable word — which the caller measures. This exists only
/// so a column of entirely empty cells still has somewhere to draw its borders.
pub(crate) const MIN_COLUMN_PT: f64 = 8.0;

/// The narrowest `available` width [`fit`] will reason about, in points.
///
/// `fit` publishes `scale` as being in `(0, 1]`, and the drawing pass relies on it:
/// `draw_table_row` **skips** the transform when the scale is not positive, so a zero or
/// negative scale draws the row at full size, off the page — the exact outcome TDD 25.17
/// exists to prevent. A non-positive `scale` has exactly one source, a non-positive
/// `available`, so the invariant is secured by flooring the input rather than by
/// clamping the output: flooring the output would keep the sign and lose the *fit*,
/// drawing a table wider than the width it was given.
///
/// The PDF sink floors its printable width at the same value before it ever calls here.
/// That is not this floor being redundant — it defends a Pango width too — and a
/// precondition a caller happens to satisfy today is not one `fit` may assume.
const MIN_AVAILABLE_PT: f64 = 1.0;

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

/// One column's two measurements, in points.
///
/// **Named so the pair cannot be transposed.** These used to be two parallel `&[f64]`
/// slices, and the twin this module transcribes — `widgets::table::layout::fit_columns`
/// — took its equivalent pair in the OPPOSITE order. The cross-check test that binds
/// the two had to hand-swap the arrays to make its "same inputs through both
/// implementations" claim true, silently and unchecked. A transposed call does not
/// panic and does not warn: every column's floor becomes its want and vice versa, so
/// the table lays out with narrow columns padded out and prose columns starved, which
/// reads as a layout opinion rather than a bug.
///
/// This project has already paid for exactly this hazard one module over —
/// `saferizer::popover_anchor` exists as a type because "transposing the two adjacent
/// integers at a call site produces a gate that is wrong but still plausible".
///
/// One slice of pairs also cannot disagree with itself about how many columns there
/// are, which retires the length-mismatch class the two-slice form carried.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ColumnWant {
    /// Max-content: everything the column holds, on one line.
    pub(crate) natural: f64,
    /// Min-content: the widest word the column cannot break.
    pub(crate) minimum: f64,
}

/// Decide column widths for one table.
///
/// `wants` is each column's [`ColumnWant`]; `available` is the printable width the table
/// has to live in. All in points.
///
/// The two measurements are what stop a squeeze from hyphen-breaking a short label: a
/// column may be offered less than it wants, but never less than one whole word of what
/// it holds, so the squeeze lands where there is prose to absorb it.
pub(crate) fn fit(wants: &[ColumnWant], available: f64, chrome: &Chrome) -> Grid {
    let count = wants.len();
    if count == 0 {
        return Grid {
            columns: Vec::new(),
            total: 0.0,
            scale: 1.0,
        };
    }

    // See MIN_AVAILABLE_PT: everything below divides by, or scales against, this width.
    let available = available.max(MIN_AVAILABLE_PT);

    // Chrome is charged per column: one border down each column's left edge, one more
    // closing the table, and padding on both sides of every cell.
    let per_column_chrome = chrome.border + chrome.padding_h * 2.0;
    let total_chrome = per_column_chrome * count as f64 + chrome.border;
    let text_available = available - total_chrome;

    // Each column's own floor: its longest unbreakable word, never below the backstop.
    let floors: Vec<f64> = wants
        .iter()
        .map(|w| {
            w.minimum
                .max(MIN_COLUMN_PT)
                // A floor may not exceed what the column actually wants, or a narrow
                // column would be padded out past its own content.
                .min(w.natural.max(MIN_COLUMN_PT))
        })
        .collect();

    // A page too narrow to hold the chrome alone: nothing sane is left to divide, so
    // fall through to the floors and let the scale regime rescue it.
    let text_widths = if text_available <= 0.0 {
        floors.clone()
    } else {
        distribute(wants, &floors, text_available)
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
fn distribute(wants: &[ColumnWant], floor: &[f64], available: f64) -> Vec<f64> {
    let count = wants.len();
    // A natural width is never less than the column's own floor.
    let natural: Vec<f64> = (0..count).map(|i| wants[i].natural.max(floor[i])).collect();

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

/// One laid-out line of a cell, in the row's unscaled text coordinates (points from the
/// top of the cell's text, padding excluded).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CellLine {
    pub(crate) top: f64,
    pub(crate) bottom: f64,
}

/// One piece of a table row, as drawn on one page.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RowSlice {
    /// Where this piece starts and ends, in the row's text coordinates. The piece's box
    /// is `bottom - top` tall plus the row's vertical padding.
    pub(crate) top: f64,
    pub(crate) bottom: f64,
    /// Per cell, the lines of that cell this piece draws — index ranges into the cell's
    /// own lines, in the order the cells were given.
    pub(crate) lines: Vec<std::ops::Range<usize>>,
}

/// Cut a table row too tall for a page into pieces, **only ever at a line boundary**
/// (TDD 25.16, 25.17).
///
/// `cells` holds each cell's lines top to bottom; `text_height` is the tallest cell's
/// height; `budget` is how much text one page holds once the row's padding is taken off,
/// all in the row's unscaled points. A row that fits — the ordinary case — comes back as
/// ONE slice holding every line, so a row is still one indivisible fragment whenever it
/// can be.
///
/// # Why a row is cut at all
///
/// A row used to be indivisible, and a row taller than a page was given a page of its
/// own and run off the foot of it: a 900-word cell kept its first 397 words and the rest
/// of the cell was silently gone. The paginator's "overflow rather than drop" rule does
/// not save a row, because what overflows a page is clipped by the media box — for one
/// line of text that is a few points, for a row it is most of the cell.
///
/// # Where it is cut
///
/// At **every** height where no line of any cell is straddled, not at the last one a page
/// could hold. The paginator then packs the pieces exactly as it packs a paragraph's
/// lines, so the row starts in whatever room the page above has left, beside its header,
/// instead of leaving that room blank and the header stranded.
///
/// Two cells whose lines never line up (different line heights in each) may share no
/// such height for longer than a page. There the cut falls at the last line top that
/// fits, in whichever cell has it; a line of another cell straddling that cut is drawn
/// whole in the piece above, a little past its box, rather than halved or lost. A single
/// line taller than the whole budget is a piece of its own, as a lone over-tall line of
/// prose is.
///
/// Every line belongs to exactly one slice — the one its top falls in. That totality is
/// what the tests assert, from the result.
pub(crate) fn split_row(cells: &[Vec<CellLine>], text_height: f64, budget: f64) -> Vec<RowSlice> {
    let mut cuts: Vec<f64> = Vec::new();
    // A non-positive budget (a page shorter than the row's own padding) cannot be cut
    // for; the row is left whole, which is what it was before cutting existed.
    if budget > 0.0 && text_height > budget {
        let mut start = 0.0_f64;
        // The latest line top since `start` that a page could still reach.
        let mut last_fit: Option<f64> = None;
        for (y, shared) in line_tops(cells, text_height) {
            if y - start > budget {
                if let Some(top) = last_fit.take() {
                    cuts.push(top);
                    start = top;
                }
                if y - start > budget {
                    // Nothing between: the piece above is one over-tall line.
                    cuts.push(y);
                    start = y;
                    continue;
                }
            }
            if shared {
                cuts.push(y);
                start = y;
                last_fit = None;
            } else {
                last_fit = Some(y);
            }
        }
        if text_height - start > budget {
            if let Some(top) = last_fit {
                cuts.push(top);
            }
        }
    }

    let bounds: Vec<f64> = std::iter::once(0.0)
        .chain(cuts.iter().copied())
        .chain(std::iter::once(text_height))
        .collect();
    let last = bounds.len() - 2;
    bounds
        .windows(2)
        .enumerate()
        .map(|(index, pair)| {
            let (top, bottom) = (pair[0], pair[1]);
            let lines = cells
                .iter()
                .map(|lines| {
                    let first = lines.iter().position(|l| l.top >= top);
                    let past = if index == last {
                        lines.len()
                    } else {
                        lines
                            .iter()
                            .position(|l| l.top >= bottom)
                            .unwrap_or(lines.len())
                    };
                    match first {
                        Some(first) if first < past => first..past,
                        _ => past..past,
                    }
                })
                .collect();
            RowSlice { top, bottom, lines }
        })
        .collect()
}

/// Every distinct line top inside the row, ascending, each paired with whether it is
/// SHARED — a height no line of any cell straddles, where every cell can break at once.
///
/// One sweep rather than a straddle test per top: a top is shared exactly when no line
/// that started above it reaches past it, which is the running maximum of the bottoms
/// seen so far. Linear after the sort, so a cell of thousands of lines costs nothing a
/// paragraph of them would not (TDD 25.22).
fn line_tops(cells: &[Vec<CellLine>], text_height: f64) -> Vec<(f64, bool)> {
    let mut all: Vec<CellLine> = cells.iter().flatten().copied().collect();
    all.sort_by(|a, b| a.top.total_cmp(&b.top));
    let mut tops: Vec<(f64, bool)> = Vec::new();
    let mut reach = 0.0_f64;
    for line in &all {
        if line.top > 0.0 && line.top < text_height {
            let shared = reach <= line.top;
            match tops.last_mut() {
                Some((y, _)) if *y == line.top => {}
                _ => tops.push((line.top, shared)),
            }
        }
        reach = reach.max(line.bottom);
    }
    tops
}

#[cfg(test)]
mod row_split_tests {
    use super::*;

    /// A cell of `count` lines of `height` each, stacked from the top.
    fn cell(count: usize, height: f64) -> Vec<CellLine> {
        (0..count)
            .map(|i| CellLine {
                top: i as f64 * height,
                bottom: (i + 1) as f64 * height,
            })
            .collect()
    }

    /// Every line of every cell lands in exactly one slice, in order — asserted from the
    /// RESULT, so it survives a change to how the cut is chosen.
    fn assert_total(cells: &[Vec<CellLine>], slices: &[RowSlice]) {
        for (c, lines) in cells.iter().enumerate() {
            let covered: Vec<usize> = slices.iter().flat_map(|s| s.lines[c].clone()).collect();
            assert_eq!(
                covered,
                (0..lines.len()).collect::<Vec<_>>(),
                "cell {c} lost, doubled or reordered a line: {slices:?}"
            );
        }
    }

    /// No line is cut: every line a slice draws lies inside that slice.
    fn assert_no_line_cut(cells: &[Vec<CellLine>], slices: &[RowSlice], budget: f64) {
        for s in slices {
            assert!(
                s.bottom - s.top <= budget || s.lines.iter().map(|r| r.len()).sum::<usize>() == 1,
                "slice {s:?} is taller than the budget {budget}"
            );
            for (c, range) in s.lines.iter().enumerate() {
                for l in &cells[c][range.clone()] {
                    assert!(
                        l.top >= s.top && l.bottom <= s.bottom,
                        "line {l:?} of cell {c} straddles slice {s:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_row_that_fits_is_one_slice_holding_every_line() {
        let cells = vec![cell(3, 10.0), cell(5, 10.0)];
        let slices = split_row(&cells, 50.0, 100.0);
        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].lines, vec![0..3, 0..5]);
        assert_eq!((slices[0].top, slices[0].bottom), (0.0, 50.0));
    }

    #[test]
    fn a_row_taller_than_the_budget_is_cut_into_pieces_that_lose_nothing() {
        // The defect: 90 lines in a 25-line page. Every line must come out somewhere.
        let cells = vec![cell(1, 10.0), cell(90, 10.0)];
        let slices = split_row(&cells, 900.0, 250.0);
        // One piece per line: every line top is a height both cells can break at, so the
        // paginator gets the finest grain it can pack.
        assert_eq!(slices.len(), 90, "{slices:?}");
        assert_total(&cells, &slices);
        assert_no_line_cut(&cells, &slices, 250.0);
        assert_eq!(
            slices[0].lines[0],
            0..1,
            "the short cell sits in the first piece"
        );
        assert!(slices[1].lines[0].is_empty(), "and nowhere else");
    }

    #[test]
    fn cells_with_different_line_pitches_are_cut_where_neither_is_halved() {
        // 10pt lines beside 14pt lines share a break height only every 70pt, so that is
        // where they are cut — never at 10, 14, 20 or 28, where one cell's line would
        // be halved.
        let cells = vec![cell(40, 10.0), cell(30, 14.0)];
        let slices = split_row(&cells, 420.0, 100.0);
        assert_total(&cells, &slices);
        assert_no_line_cut(&cells, &slices, 100.0);
        assert_eq!(slices[0].bottom, 70.0, "{slices:?}");
        // Five more at 70 apart, then 406 — past the 10pt cell's last line, so every
        // later 14pt line top is a break both cells can take.
        assert_eq!(slices.len(), 7, "{slices:?}");
    }

    #[test]
    fn a_single_line_taller_than_the_budget_is_kept_whole_rather_than_halved() {
        let cells = vec![vec![
            CellLine {
                top: 0.0,
                bottom: 10.0,
            },
            CellLine {
                top: 10.0,
                bottom: 300.0,
            },
            CellLine {
                top: 300.0,
                bottom: 310.0,
            },
        ]];
        let slices = split_row(&cells, 310.0, 100.0);
        assert_total(&cells, &slices);
        assert_no_line_cut(&cells, &slices, 100.0);
        assert_eq!(
            slices[1].lines[0],
            1..2,
            "the tall line stands alone: {slices:?}"
        );
    }

    #[test]
    fn cells_that_never_line_up_are_still_cut_and_lose_nothing() {
        // 10pt beside 10.3pt lines share no break height for far longer than a page,
        // so the cut falls at the last line top that fits rather than not at all.
        let cells = vec![cell(100, 10.0), cell(97, 10.3)];
        let slices = split_row(&cells, 1000.0, 100.0);
        assert_total(&cells, &slices);
        assert!(slices.len() >= 10, "{slices:?}");
        for s in &slices {
            assert!(s.bottom - s.top <= 100.0, "an over-budget piece: {s:?}");
        }
    }

    #[test]
    fn a_line_shared_by_two_cells_is_one_cut_not_two() {
        let cells = vec![cell(30, 10.0), cell(30, 10.0)];
        let slices = split_row(&cells, 300.0, 100.0);
        assert_total(&cells, &slices);
        assert_eq!(slices.len(), 30, "{slices:?}");
    }

    #[test]
    fn a_non_positive_budget_leaves_the_row_whole() {
        let cells = vec![cell(10, 10.0)];
        let slices = split_row(&cells, 100.0, 0.0);
        assert_eq!(slices.len(), 1);
        assert_total(&cells, &slices);
    }
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

    /// Pair naturals with minimums for a test. Takes them in `fit`'s own documented
    /// order (natural, then minimum) and produces the pairs, so a test cannot express
    /// the transposition this type exists to prevent.
    fn wants(natural: &[f64], minimum: &[f64]) -> Vec<ColumnWant> {
        assert_eq!(natural.len(), minimum.len(), "fixture length mismatch");
        natural
            .iter()
            .zip(minimum)
            .map(|(&natural, &minimum)| ColumnWant { natural, minimum })
            .collect()
    }

    fn widths(grid: &Grid) -> Vec<f64> {
        grid.columns.iter().map(|c| c.text_width).collect()
    }

    /// Run the same `(natural, minimum)` shapes through the preview's `fit_columns`
    /// (`i32` px) and this file's `fit` (`f64` pt), taking each pair in `fit`'s own
    /// documented order and converting it to the preview's OPPOSITE order internally —
    /// so no caller of this helper can express the transposition [`ColumnWant`]'s own
    /// docs warn about. Returns `(preview widths, export widths)` for the caller to
    /// compare.
    fn preview_vs_export(columns: &[(i32, i32)], bound: i32) -> (Vec<i32>, Vec<f64>) {
        let preview = crate::widgets::table::layout::fit_columns(
            &columns
                .iter()
                .map(
                    |&(natural, minimum)| crate::widgets::table::layout::ColumnWant {
                        natural,
                        minimum,
                    },
                )
                .collect::<Vec<_>>(),
            bound,
        );
        let ours = widths(&fit(
            &columns
                .iter()
                .map(|&(natural, minimum)| ColumnWant {
                    natural: f64::from(natural),
                    minimum: f64::from(minimum),
                })
                .collect::<Vec<_>>(),
            f64::from(bound),
            &bare(),
        ));
        (preview, ours)
    }

    #[test]
    fn a_table_that_fits_keeps_every_columns_natural_width() {
        let grid = fit(
            &wants(&[100.0, 50.0, 60.0], &[20.0, 20.0, 20.0]),
            500.0,
            &chrome(),
        );
        assert_eq!(widths(&grid), vec![100.0, 50.0, 60.0]);
        assert_eq!(grid.scale, 1.0, "a table that fits is never scaled");
    }

    #[test]
    fn a_narrow_table_is_not_stretched_to_the_margin() {
        // Regime 1 leaves slack rather than spreading three short columns across a
        // whole page, which is what the preview does and what a reader expects.
        let grid = fit(
            &wants(&[20.0, 20.0, 20.0], &[10.0, 10.0, 10.0]),
            500.0,
            &chrome(),
        );
        assert!(
            grid.total < 200.0,
            "a narrow table grew to {}pt of a 500pt page",
            grid.total
        );
    }

    #[test]
    fn columns_are_laid_end_to_end_with_no_gap_and_no_overlap() {
        let grid = fit(
            &wants(&[100.0, 50.0, 60.0], &[20.0, 20.0, 20.0]),
            500.0,
            &chrome(),
        );
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
        let grid = fit(
            &wants(&[200.0, 100.0, 100.0], &[10.0, 10.0, 10.0]),
            200.0,
            &bare(),
        );
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
        let grid = fit(
            &wants(&[600.0, 60.0, 60.0], &[40.0, 55.0, 55.0]),
            300.0,
            &bare(),
        );
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
    fn the_declared_divergences_hold_in_regimes_1_and_3() {
        // CAM row 17: the export's columns must be the preview's columns, EXCEPT where
        // the module header declares a divergence. Regime 2 (the shared rule) is a
        // property that needs many inputs to be trusted, and is checked by the sweep
        // below (`the_shared_rule_holds_across_a_sweep_of_overflow_shapes`); regimes 1
        // and 3 are declared DIVERGENCES, not a shared rule, so one representative shape
        // per regime is the right amount of evidence — asserted so the day someone
        // "fixes" one for symmetry, the test names the decision they overturned.
        let columns: [(i32, i32); 3] = [(600, 40), (60, 55), (60, 55)]; // (natural, minimum)

        // REGIME 1 — everything fits. The preview stretches to the bound; the page keeps
        // natural widths. A DECLARED divergence, asserted so it cannot be silently lost.
        let (preview, ours) = preview_vs_export(&columns, 5000);
        let preview_total: i32 = preview.iter().sum();
        let ours_total: f64 = ours.iter().sum();
        assert!(
            f64::from(preview_total) > ours_total + 1.0,
            "the preview must fill its pane where the page does not: \
             preview {preview:?} vs export {ours:?}"
        );
        for (want, mine) in columns.iter().zip(&ours) {
            assert!(
                (f64::from(want.0) - mine).abs() <= 1.0,
                "the page keeps natural widths when everything fits: {ours:?}"
            );
        }

        // REGIME 3 — not even the floors fit. The preview stops at sum(min) and scrolls;
        // the page scales the whole table down onto the bound.
        let narrow: [(i32, i32); 3] = [(600, 200), (400, 150), (300, 120)];
        let (preview, _) = preview_vs_export(&narrow, 100);
        let preview_total: i32 = preview.iter().sum();
        assert!(
            preview_total > 100,
            "the preview overflows and scrolls in regime 3: {preview:?}"
        );
        let grid = fit(
            &narrow
                .iter()
                .map(|&(natural, minimum)| ColumnWant {
                    natural: f64::from(natural),
                    minimum: f64::from(minimum),
                })
                .collect::<Vec<_>>(),
            100.0,
            &bare(),
        );
        assert!(
            grid.scale < 1.0 && grid.drawn_width() <= 100.01,
            "the page scales onto the bound in regime 3: scale {} drawn {}",
            grid.scale,
            grid.drawn_width()
        );
    }

    #[test]
    fn the_shared_rule_holds_across_a_sweep_of_overflow_shapes() {
        // Regime 2 (scarce but sufficient) is the ONLY regime the two implementations
        // are meant to agree on — see the module header. A single hand-picked shape
        // (this test's own history, M34) cannot tell "the rule holds" from "these two
        // inputs happen to coincide", so this sweeps column counts, want-shapes and
        // bounds, filtered to the water-fill branch in BOTH implementations
        // (sum(minimum) < bound < sum(natural)).
        //
        // The check is agreement within a pixel and never-below-floor, not bit-for-bit
        // equality: the two loops reach the shared width by genuinely different
        // arithmetic (this file's one-shot continuous division vs. the preview's
        // per-step integer-truncating division — see the module header's account of why
        // a shared primitive was rejected), so exact per-column equality is not a
        // property that should hold, and asserting it would make this test lie about
        // what "the shared rule" actually guarantees.
        for ncols in 2..=5_usize {
            for shape in 0..4_usize {
                let naturals: Vec<i32> = (0..ncols)
                    .map(|i| 30 + (i as i32 + 1) * (20 + shape as i32 * 15))
                    .collect();
                let minimums: Vec<i32> = naturals
                    .iter()
                    .map(|&n| (n / (2 + shape as i32)).max(5))
                    .collect();
                let sum_min: i32 = minimums.iter().sum();
                let sum_nat: i32 = naturals.iter().sum();
                let columns: Vec<(i32, i32)> = naturals
                    .iter()
                    .zip(&minimums)
                    .map(|(&n, &m)| (n, m))
                    .collect();

                let mut bound = sum_min + 1;
                while bound < sum_nat {
                    let (preview, ours) = preview_vs_export(&columns, bound);
                    assert_eq!(
                        preview.len(),
                        ours.len(),
                        "column count diverged at ncols={ncols} shape={shape} bound={bound}"
                    );

                    let preview_sum: i32 = preview.iter().sum();
                    let ours_sum: f64 = ours.iter().sum();
                    assert!(
                        preview_sum <= bound,
                        "preview exceeded the bound at ncols={ncols} shape={shape} \
                         bound={bound}: {preview:?}"
                    );
                    assert!(
                        ours_sum <= f64::from(bound) + 0.01,
                        "export exceeded the bound at ncols={ncols} shape={shape} \
                         bound={bound}: {ours:?}"
                    );

                    for (i, (&p, &o)) in preview.iter().zip(&ours).enumerate() {
                        let floor = f64::from(minimums[i]);
                        assert!(
                            f64::from(p) >= floor - 0.01,
                            "preview column {i} fell below its floor at ncols={ncols} \
                             shape={shape} bound={bound}: {preview:?}"
                        );
                        assert!(
                            o >= floor - 0.01,
                            "export column {i} fell below its floor at ncols={ncols} \
                             shape={shape} bound={bound}: {ours:?}"
                        );
                        assert!(
                            (f64::from(p) - o).abs() <= 1.0,
                            "regime 2 is the shared rule and must agree at ncols={ncols} \
                             shape={shape} bound={bound}, column {i}: preview {p} vs \
                             export {o}"
                        );
                    }

                    // Determinism: the same inputs must produce the same outputs.
                    let (preview_again, ours_again) = preview_vs_export(&columns, bound);
                    assert_eq!(preview, preview_again, "preview is non-deterministic");
                    assert_eq!(ours, ours_again, "export is non-deterministic");

                    bound += 7;
                }
            }
        }
    }

    #[test]
    fn the_column_count_survives_any_squeeze() {
        // TDD 25.17's "never reflowed into a different table": the structure is the
        // invariant, at every width including absurd ones.
        for available in [1.0, 10.0, 80.0, 200.0, 1000.0] {
            let grid = fit(
                &wants(&[300.0, 200.0, 120.0, 90.0], &[40.0, 30.0, 25.0, 20.0]),
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
        let grid = fit(
            &wants(&[600.0, 60.0, 60.0], &[40.0, 55.0, 55.0]),
            300.0,
            &bare(),
        );
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
        let grid = fit(&wants(&[12.0, 400.0], &[12.0, 40.0]), 200.0, &bare());
        let w = widths(&grid);
        assert!(w[0] <= 12.0 + f64::EPSILON, "narrow column inflated: {w:?}");
    }

    #[test]
    fn scale_to_fit_is_the_last_resort_and_lands_on_the_page() {
        // Six columns whose longest words alone overflow 120pt: wrapping cannot save
        // it, so regime 3 fires and the drawn width is exactly the page —
        // TDD 25.17: scaled, never clipped.
        let grid = fit(&wants(&[200.0; 6], &[60.0; 6]), 120.0, &chrome());
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
        let grid = fit(&wants(&[900.0, 900.0], &[30.0, 30.0]), 300.0, &bare());
        assert_eq!(grid.scale, 1.0, "scaled when wrapping would have done");
    }

    #[test]
    fn a_scale_is_never_an_upscale() {
        // A table with room to spare is left alone, not blown up to fill the page.
        let grid = fit(&wants(&[10.0, 10.0], &[10.0, 10.0]), 600.0, &chrome());
        assert_eq!(grid.scale, 1.0);
    }

    #[test]
    fn a_table_with_no_columns_is_geometry_free_rather_than_a_panic() {
        let grid = fit(&wants(&[], &[]), 500.0, &chrome());
        assert!(grid.columns.is_empty());
        assert_eq!(grid.total, 0.0);
        assert_eq!(grid.scale, 1.0);
    }

    #[test]
    fn a_page_narrower_than_the_chrome_still_produces_a_drawable_grid() {
        // Degenerate, but reachable through a hostile page setup: every width must
        // stay finite and positive or the drawing pass emits nothing at all.
        let grid = fit(&wants(&[100.0, 100.0], &[20.0, 20.0]), 3.0, &chrome());
        assert!(grid.scale > 0.0 && grid.scale <= 1.0);
        assert!(grid.columns.iter().all(|c| c.text_width > 0.0));
        assert!(grid.drawn_width() <= 3.01);
    }

    #[test]
    fn a_column_of_empty_cells_still_has_a_drawable_width() {
        let grid = fit(&wants(&[0.0, 100.0], &[0.0, 20.0]), 200.0, &bare());
        assert!(
            widths(&grid)[0] >= MIN_COLUMN_PT,
            "an empty column collapsed to nothing"
        );
    }

    #[test]
    fn every_width_respects_its_invariants_across_a_sweep_of_shapes() {
        // A property sweep rather than another example: the arithmetic has three
        // regimes and a pinning loop, and this one `available` list already reaches all
        // three (fits-outright at 300/5000, overflow at 47, floors-don't-fit at -5/0/0.5/5)
        // so one sweep verifies finiteness AND the floor/bound contract in every regime.
        for columns in 1..8_usize {
            // The list starts below zero on purpose. It used to start at 0.5, and that
            // is why nothing caught `available <= 0` returning a scale of 0.0 or -0.085:
            // a caller reaches those by indenting a table further than the page is wide.
            for available in [-5.0, 0.0, 0.5, 5.0, 47.0, 300.0, 5000.0] {
                let natural: Vec<f64> = (0..columns).map(|i| 10.0 + i as f64 * 90.0).collect();
                let minimum: Vec<f64> = natural.iter().map(|n| n / 3.0).collect();
                let grid = fit(&wants(&natural, &minimum), available, &chrome());
                assert_eq!(grid.columns.len(), columns);
                assert!(
                    grid.scale.is_finite() && grid.scale > 0.0 && grid.scale <= 1.0,
                    "scale {} at {columns} columns / {available}pt",
                    grid.scale
                );
                // The same floor `fit` itself computes, so this checks its output
                // against its own contract rather than a re-derivation that could drift.
                let floors: Vec<f64> = natural
                    .iter()
                    .zip(&minimum)
                    .map(|(&n, &m)| m.max(MIN_COLUMN_PT).min(n.max(MIN_COLUMN_PT)))
                    .collect();
                for (column, &floor) in grid.columns.iter().zip(&floors) {
                    assert!(
                        column.text_width.is_finite() && column.text_width > 0.0,
                        "width {} at {columns} columns / {available}pt",
                        column.text_width
                    );
                    assert!(column.x.is_finite() && column.x >= 0.0);
                    assert!(
                        column.text_width >= floor - 1e-6,
                        "column fell below its floor ({floor}) at {columns} columns / \
                         {available}pt: {}",
                        column.text_width
                    );
                }
                assert!(
                    grid.drawn_width() <= available.max(MIN_AVAILABLE_PT) + 1e-6,
                    "drawn width {} exceeded the bound {available}pt at {columns} columns",
                    grid.drawn_width()
                );
            }
        }
    }
}
