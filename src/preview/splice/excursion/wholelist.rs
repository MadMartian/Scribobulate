//! **A measurement, not a feature guard** — the third one, and it exists to resolve a
//! CONFOUND in the two before it rather than to test a new mechanism.
//!
//! # The confound
//!
//! [`super::drift`] establishes that expanding a disclosure above the reader drifts
//! their content in proportion to the NUMBER of anchored children involved, and not to
//! those children's height (the same 33.6 px for a 27 px separator and a 50 px image).
//! What it cannot say is *which* number. Every anchored child its fixture creates lives
//! inside the toggled body, and the only other one in the whole view is the
//! disclosure's own toggle button — so "the children the toggled REGION draws" and
//! "every anchored child in the WHOLE VIEW" are the same count plus one, and they move
//! together. No cell of that table can attribute the per-child cost to one rather than
//! the other.
//!
//! The distinction is not academic. A candidate mechanism reads on `gtktextview.c`'s
//! `changed_handler` (4.6.9, around `:4926`), which runs an UNCONDITIONAL loop over
//! `priv->anchored_children` — every anchored child in the view, not the changed range's
//! — on every height-delta `::changed`. If the per-child term is that loop, it is a
//! property of the VIEW's population and a splice can do nothing about it by rendering
//! fewer children; if it is the region's own, it is a property of what the toggle drew.
//!
//! # The knob
//!
//! [`super::harness::tall_document_with_body_and_tail_children`] adds a second,
//! independent population of anchored children in the TAIL — below the disclosure and
//! outside the range a toggle deletes. A tail child is in the view's list and not in
//! the region: it survives the toggle as the same already-parented, already-laid-out
//! widget object. So `M` moves the whole-view count while leaving the region count at
//! `N`, and the two hypotheses make opposite predictions:
//!
//! * drift tracking `N + M` — the whole-list loop is the count term.
//! * drift tracking `N` alone, with `M` inert — the region is, and the whole-list loop
//!   is out.
//!
//! # What it measured (GTK 4.6.9, X11/Xvfb, 700x600 pane, deferred parenting, expanding)
//!
//! | body `N` | tail `M` | live anchors before | drift | height the toggle added |
//! |---|---|---|---|---|
//! | 0 | 0 | 1 | +32 px | +2 160 px |
//! | 10 | 0 | 1 | **+368 px** | +2 430 px |
//! | 0 | 10 | 11 | +32 px | +2 160 px |
//! | 10 | 10 | 11 | **+368 px** | +2 430 px |
//! | 0 | 30 | 31 | +32 px | +2 160 px |
//!
//! Per child: a BODY child costs **33.6 px** whether the tail holds 0 or 10 others; a
//! TAIL child costs **0.0 px** at both 10 and 30. The `live anchors` column is `1 + M`
//! exactly — the one constant being the disclosure's own toggle button — so the knob
//! delivered its dose, and the `height` column is a function of `N` alone, confirming
//! the tail children sit outside what the toggle inserts.
//!
//! **`M` HAS NO EFFECT — a null result, and it is the informative one.** Thirty extra
//! anchored children in the view, all above the reading position, all in the same
//! `priv->anchored_children` list the candidate loop walks, cost the reader nothing:
//! the drift at `(0, 30)` is the zero-child base to the pixel. The per-child term is
//! carried entirely by the children the toggled REGION draws — `(10, 0)` and `(10, 10)`
//! both cost 33.6 px each — and is indifferent to how many anchored children the view
//! already held. The whole-list loop is therefore not the count term.
//!
//! What that does NOT settle: it says nothing about how many times the loop RUNS, only
//! that its LENGTH does not enter the drift. [`super::trace`] is the other half.

use super::harness::{
    measure_probed, splice_toggle, tall_document_with_body_and_tail_children, Arm, Parenting, Phase,
};

/// The grid. `(0, 0)` is the base every excess is measured against; `(10, 0)` is the
/// positive control (the documented per-child dose-response, which must reproduce or
/// nothing else here means anything); `(0, 10)` and `(0, 30)` move the view's whole
/// list with the region empty, which is the discriminating cell; `(10, 10)` checks
/// that a loaded list does not change what a loaded region costs.
const GRID: [(usize, usize); 5] = [(0, 0), (10, 0), (0, 10), (10, 10), (0, 30)];

/// The Markdown construct that renders as exactly one anchored widget. A thematic
/// break, the same child [`super::drift`] takes its headline figure with, so the two
/// tables' per-child numbers are directly comparable.
const RULE: &str = "---";

/// Slack for every shape assertion here, in pixels — about one text row on this rig,
/// the same bound and the same reasoning as [`super::drift`]'s. Everything asserted is
/// a SHAPE (is there a dose-response at all; did the tail population move anything),
/// so the tolerance only has to be small against a per-child cost of tens of pixels.
const SLACK_PX: f64 = 12.0;

/// One cell of the grid.
struct Cell {
    body: usize,
    tail: usize,
    /// How many anchored children the view held immediately before the toggle —
    /// the proof that `M` actually arrived. Read at [`Phase::BeforeToggle`] rather
    /// than derived from the fixture string, so a cell whose tail children silently
    /// failed to render cannot report a dose it never received (ScrAP-252's family:
    /// a setup step that does not take effect makes the next assertion answer for
    /// the previous state).
    live_anchors_before: usize,
    arm: Arm,
}

impl Cell {
    fn drift(&self) -> f64 {
        self.arm.content_drift_px()
    }

    fn upper_delta(&self) -> f64 {
        self.arm.after.upper - self.arm.before.upper
    }
}

/// Measure one cell: build the fixture, park the reader, toggle through the splice,
/// read the drift.
///
/// Deferred parenting throughout — [`super::drift`] measured the eager arm identical
/// to the pixel at every count, so re-running it here would double the cost of this
/// table to re-establish a settled negative.
fn cell(body: usize, tail: usize) -> Cell {
    let md = tall_document_with_body_and_tail_children(body, tail, RULE);
    let drawn = std::cell::Cell::new(0usize);
    let live_anchors_before = std::cell::Cell::new(0usize);
    let arm = measure_probed(
        "splice",
        &md,
        false,
        |rig, phase| {
            if phase == Phase::BeforeToggle {
                live_anchors_before.set(rig.anchored.len());
            }
        },
        |rig, folds, key| {
            drawn.set(splice_toggle(rig, folds, key, &md, Parenting::Deferred));
        },
    );

    // The body dose arrived: the region render really drew the children this cell
    // claims to have measured.
    assert!(
        drawn.get() >= body,
        "the fixture asked for {body} anchored children in the disclosure body and \
         the region render drew only {} anchored widgets, so this cell measures a \
         smaller dose than it reports",
        drawn.get(),
    );
    // The tail dose arrived at all. The exact count is checked ACROSS cells below,
    // which is the stronger test; this only catches a fixture that rendered none.
    assert!(
        live_anchors_before.get() >= tail,
        "the fixture asked for {tail} anchored children in the tail and the view held \
         only {} anchored children before the toggle — the tail knob did not take \
         effect and this cell measures the same view as the zero-tail one",
        live_anchors_before.get(),
    );
    assert!(
        arm.anchor_survived(),
        "the splice destroyed the reader's anchor at body {body} / tail {tail}, so the \
         drift measured for this cell describes some other line"
    );

    Cell {
        body,
        tail,
        live_anchors_before: live_anchors_before.get(),
        arm,
    }
}

/// The cell at `(body, tail)`, which the grid is a constant so is always present.
fn at(cells: &[Cell], body: usize, tail: usize) -> &Cell {
    cells
        .iter()
        .find(|c| c.body == body && c.tail == tail)
        .expect("the grid measured this cell")
}

/// The per-child cost of `count` BODY children, at a fixed tail population — the
/// figure [`super::drift`] reports, isolated by removing the same tail's own base.
fn per_body_child(cells: &[Cell], count: usize, tail: usize) -> f64 {
    (at(cells, count, tail).drift() - at(cells, 0, tail).drift()) / count as f64
}

/// The per-child cost of `count` TAIL children, at an empty region — the discriminator.
fn per_tail_child(cells: &[Cell], count: usize) -> f64 {
    (at(cells, 0, count).drift() - at(cells, 0, 0).drift()) / count as f64
}

/// **The measurement.** The whole grid in one test, deliberately: the base cell, the
/// positive control and the discriminating cell are only meaningful against each
/// other, and split across tests a filtered run could report the discriminator alone.
#[gtktest::test]
fn tail_anchored_children_outside_the_region_do_not_move_the_reader() {
    if super::skip_if_gtk_compensates_top_margin("drift: whole-list loop") {
        return;
    }
    let cells: Vec<Cell> = GRID.iter().map(|&(b, t)| cell(b, t)).collect();

    let mut report = String::from(
        "\n=== per-child drift: region children (N) against whole-view children (N+M) ===\n\
         \x20 expanding a disclosure ABOVE the reading position, deferred parenting\n\
         \x20 {N} = anchored children in the toggled BODY, {M} = in the TAIL (outside \
         the region)\n\
         \x20 N is what the region DRAWS; N+M is what the view's anchored_children \
         list HOLDS\n\n",
    );
    report.push_str(&format!(
        "  {:>3} {:>4} {:>16} {:>10} {:>10}\n",
        "N", "M", "live anchors", "drift", "height"
    ));
    for c in &cells {
        report.push_str(&format!(
            "  {:>3} {:>4} {:>16} {:>8.0}px {:>8.0}px\n",
            c.body,
            c.tail,
            c.live_anchors_before,
            c.drift(),
            c.upper_delta(),
        ));
    }
    report.push_str(&format!(
        "\n  per BODY child, empty tail   (N=10, M=0)  : {:>6.1}px\n  \
         per BODY child, loaded tail  (N=10, M=10) : {:>6.1}px\n  \
         per TAIL child, empty region (N=0,  M=10) : {:>6.1}px\n  \
         per TAIL child, empty region (N=0,  M=30) : {:>6.1}px\n",
        per_body_child(&cells, 10, 0),
        per_body_child(&cells, 10, 10),
        per_tail_child(&cells, 10),
        per_tail_child(&cells, 30),
    ));
    println!("{report}");

    // ── The tail knob is real, and by exactly the dose asked for. ──────────────
    //
    // Across cells rather than within one: the absolute count includes the
    // disclosure's own toggle button, which belongs to the summary line and is not
    // this fixture's business. The DIFFERENCE is, and it is exact.
    for tail in [10, 30] {
        let added = at(&cells, 0, tail).live_anchors_before - at(&cells, 0, 0).live_anchors_before;
        assert_eq!(
            added, tail,
            "the tail knob is meant to add exactly {tail} anchored children to the \
             view and added {added}, so the grid's M column does not mean what it \
             says.{report}"
        );
    }

    // ── The positive control, first. ───────────────────────────────────────────
    let control = per_body_child(&cells, 10, 0);
    assert!(
        control > SLACK_PX,
        "THE RIG IS NOT EXERCISING THE PHENOMENON. Ten anchored children in the \
         toggled body are documented to drift the reader by tens of pixels each \
         (MEASURED 33.6px), and on this rig each cost only {control:.1}px. Every other \
         number here is therefore meaningless — a null result for the tail population \
         with no positive control for the body one is a statement about the fixture, \
         not about GTK.{report}"
    );

    // ── The finding: the tail population is inert. ─────────────────────────────
    //
    // Stated as a NULL — the tail children cost nothing that scales — because that is
    // the measured result and it is what rules the whole-list loop out as the count
    // term. Asserted at both tail counts, since a per-child figure that hid inside
    // one count's noise could not hide inside thirty.
    for tail in [10, 30] {
        let per_tail = per_tail_child(&cells, tail);
        assert!(
            per_tail.abs() <= SLACK_PX,
            "anchored children OUTSIDE the toggled region have started to cost the \
             reader ({per_tail:.1}px each at M={tail}, against {control:.1}px for a \
             child the region actually draws). This test records the opposite — that \
             the view's whole anchored_children population is inert and only the \
             region's own children drift the reader — so a cost here is a change to \
             the finding: re-read the table above and update this module's docs \
             rather than widening the tolerance.{report}"
        );
    }

    // ── And a loaded list does not change what a loaded region costs. ──────────
    let loaded = per_body_child(&cells, 10, 10);
    assert!(
        (loaded - control).abs() <= SLACK_PX,
        "the per-child cost of the REGION's children has started to depend on how \
         many anchored children the view already held ({loaded:.1}px each against a \
         tenfold-emptier view's {control:.1}px), which is the interaction this grid \
         records as absent.{report}"
    );
}
