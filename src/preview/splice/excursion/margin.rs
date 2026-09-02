//! **A measurement, not a feature guard** — the fifth, and the one that turns a knob
//! rather than adding a dose.
//!
//! # The model under test
//!
//! [`super::trace`] establishes that the drift is a shortfall in what the compensating
//! `vadjustment` writes carried, and that their number tracks the anchored children the
//! toggled region drew. A reading of `gtktextview.c` 4.6.9 proposes what each is short BY:
//!
//! ```text
//! drift = (number of compensating ::changed emissions) x top_margin
//! ```
//!
//! `changed_handler` (`:4918-4925`) computes `priv->yoffset += new_first_para_top -
//! old_first_para_top` and hands the result to `gtk_adjustment_set_value`, while
//! everywhere else in that file the invariant is `yoffset == value - top_margin` — so
//! each compensating pass lands `top_margin` px short, once per pass. Upstream fixed
//! exactly this in 4.19.3 (`b3006986297e`, "textview: fix yoffset position when
//! top_margin is set", GNOME/gtk#4134) by adding `+ priv->top_margin`; 4.6.9 has not.
//!
//! # The knob, and why it is on the rig's view
//!
//! `top-margin` is a per-view widget property, so this overrides it on the rig's own
//! view ([`super::rig::Rig::new`]) and never in `config.rs` — changing a shipped
//! default to take a measurement is how a measurement leaks into an artefact. The
//! margin standing on the view is read back off it at the moment of the toggle and
//! printed in every row, so the knob is PROVED to have taken rather than assumed
//! (ScrAP-252's family: a setup step that silently does not apply makes the next
//! reading answer for the previous state).
//!
//! Three margins: zero, the configured one, and three times it. The middle arm is the
//! POSITIVE CONTROL and is load-bearing — it must reproduce [`super::trace`]'s and
//! [`super::drift`]'s tables in the same run, or the rig is not exercising the
//! phenomenon and no other cell means anything.
//!
//! # What it measured (GTK 4.6.9, X11/Xvfb, 700x600 pane, deferred parenting)
//!
//! **The identity holds EXACTLY, in all eighteen cells — residual 0.0 px everywhere.**
//! Expanding, with the thematic-break dose:
//!
//! | `top_margin` | children | emissions | sum(deltas) | height added | drift | `emissions x tm` |
//! |---|---|---|---|---|---|---|
//! | 0 | 0 | 2 | +2 160 | +2 160 | **0** | 0 |
//! | 0 | 10 | 23 | +2 430 | +2 430 | **0** | 0 |
//! | 0 | 30 | 55 | +2 970 | +2 970 | **0** | 0 |
//! | 16 | 0 | 2 | +2 128 | +2 160 | **+32** | 32 |
//! | 16 | 10 | 23 | +2 062 | +2 430 | **+368** | 368 |
//! | 16 | 30 | 55 | +2 090 | +2 970 | **+880** | 880 |
//! | 48 | 0 | 2 | +2 064 | +2 160 | **+96** | 96 |
//! | 48 | 10 | 23 | +1 326 | +2 430 | **+1 104** | 1 104 |
//! | 48 | 30 | 55 | +330 | +2 970 | **+2 640** | 2 640 |
//!
//! (The thirty-child expanding count is bimodal run to run — last section. Each row is
//! one run; the identity held in every run of both modes.)
//!
//! Collapsing, same shape: 1 / 2 / 2 emissions at every margin, drift **0 / 0 / 0** at
//! zero, **+16 / +32 / +32** at sixteen, **+48 / +96 / +96** at forty-eight. The `16` rows
//! reproduce [`super::trace`]'s and [`super::drift`]'s tables to the pixel and to the
//! emission — the positive control, taken in the same run.
//!
//! **EVERY LOGGED DELTA MOVES BY EXACTLY THE MARGIN'S OWN CHANGE**, which is the
//! primary evidence; the drift is only its accumulation. Against the `16` control, at
//! every dose and in both directions, each delta sits at exactly `+16` when the margin
//! is 0 and exactly `−32` when it is 48 — a bigger margin makes every compensating
//! write smaller, one for one. At ten children:
//!
//! * `tm = 0`: `10 × +9.0`, `1 × −72.0`, `10 × +90.0`, `1 × +1152.0`, `1 × +360.0`
//! * `tm = 16`: `10 × −7.0`, `1 × −88.0`, `10 × +74.0`, `1 × +1136.0`, `1 × +344.0`
//! * `tm = 48`: `10 × −39.0`, `1 × −120.0`, `10 × +42.0`, `1 × +1104.0`, `1 × +312.0`
//!
//! Two riders the margin does NOT touch: the height the toggle adds is identical at all
//! three margins (+2 160 / +2 430 / +2 970), and at `tm = 0` the compensation carries it
//! EXACTLY — `sum(deltas) == height added` in all six cells, so a zero margin is not
//! merely a smaller drift but a complete one.
//!
//! # The emission count is NOT deterministic, and the identity holds through it
//!
//! Run five times, the thirty-child EXPANDING cell comes back with **55 or 63**
//! emissions, and which is not a function of the margin — 63 appeared at 48 alone in one
//! run, at 16 alone in another, at both in a third, and in neither of the other two. The
//! modes agree up to the saturation, then differ in how the remainder is carried:
//!
//! * 55: `30 × −7.0`, `1 × −88.0`, `22 × +74.0`, `1 × +56.0`, `1 × +704.0`
//! * 63: the same first four runs, then `1 × +2.0`, `7 × +74.0`, `1 × +56.0`
//! — the up-leg either stops at 22 with one bulk write carrying the rest, or resumes for
//! a further seven. **Why it does either is now MEASURED** ([`super::kink`],
//! [`super::budget`]): the up-leg is one `gtk_text_layout_validate` pass spending a
//! fixed pixel budget and the resumption is a SECOND pass, so the modes are
//! `N + min(N, 22) + 3` and `2N + 3`. Recorded here because it MOVES THE READER: the same toggle at the configured margin drifts them 880 px in one mode and
//! 1 008 px in the other, with nothing whatever changed between the runs.
//!
//! **The identity is untouched by it, and that is the strongest evidence here rather
//! than a caveat** — 55 × 16 = 880 and 63 × 16 = 1 008, both to the pixel, so the drift
//! tracks a count that varies on its OWN. A term that follows an uncontrolled input
//! exactly is not an artefact of one fixture. The corollary a reader might reach for is
//! the only casualty: "triple the margin, triple the drift" holds cell for cell only
//! while the count holds, so a drift predicted from a margin needs the count MEASURED
//! rather than carried over.

use gtk::prelude::*;

use super::harness::{
    measure_probed_at_margin, splice_toggle, tall_document_with_children, Arm, Parenting, Phase,
    ZOOM,
};
use super::recorder::{self, Emission, Trace};

/// The dose axis, unchanged from [`super::drift`] so the two tables are comparable.
const CHILD_COUNTS: [usize; 3] = [0, 10, 30];

/// The Markdown construct that renders as exactly one anchored widget — the thematic
/// break every other experiment on this rig doses with.
const RULE: &str = "---";

/// Slack for the positive control, in pixels — about one text row, as elsewhere here.
const SLACK_PX: f64 = 12.0;

/// Tolerance for the identity below, in pixels. Deliberately far tighter than
/// [`SLACK_PX`]: this is an EQUALITY between whole-pixel quantities and not a shape, so
/// the only error to absorb is one multiplication's. A pixel of slack here would let a
/// second term hide inside the one being measured.
const IDENTITY_EPSILON_PX: f64 = 1e-9;

/// The margin the application configures, derived the way
/// [`crate::preview::build::apply_preview_margins`] derives it rather than restated as a
/// literal — a second copy is how the control arm stops being the shipped value.
pub(super) fn configured_top_margin() -> i32 {
    crate::theme::px(crate::config::config().view.top_margin, ZOOM)
}

/// The three margins, as multiples of the configured one.
///
/// Zero and triple rather than two arbitrary numbers: the model predicts a drift LINEAR
/// in the margin through the origin, so zero is where it predicts the drift vanishes
/// outright and triple where it predicts every cell multiplies by exactly three — two
/// predictions a near-miss cannot satisfy.
fn margins() -> [i32; 3] {
    let configured = configured_top_margin();
    [0, configured, configured * 3]
}

/// One cell of the grid.
struct Cell {
    /// The margin the rig was ASKED for.
    asked: i32,
    /// The margin the live view REPORTED at the moment of the toggle.
    observed: i32,
    children: usize,
    expanding: bool,
    arm: Arm,
    emissions: Vec<Emission>,
}

impl Cell {
    /// How far the reader's content moved on screen — the quantity the model predicts.
    fn drift(&self) -> f64 {
        self.arm.content_drift_px()
    }

    /// How far the adjustment ended up from where it started.
    fn value_delta(&self) -> f64 {
        self.arm.after.value - self.arm.before.value
    }

    /// How much document the toggle put ABOVE the reader's marked line — their line moved
    /// by `drift` and the viewport under it by `value_delta`, so what went in above them
    /// is the sum. A perfect compensation makes the two equal and the drift zero.
    fn height_added(&self) -> f64 {
        self.drift() + self.value_delta()
    }

    fn sum_of_deltas(&self) -> f64 {
        recorder::sum_of_deltas(&self.emissions)
    }

    /// What the model above predicts this cell's drift to be.
    fn predicted_drift(&self) -> f64 {
        self.emissions.len() as f64 * f64::from(self.observed)
    }
}

/// Measure one cell: build the fixture at `top_margin`, park the reader, toggle through
/// the splice, and record every `value-changed` across the settle.
fn cell(top_margin: i32, children: usize, expanding: bool) -> Cell {
    let md = tall_document_with_children(children, RULE);
    let recorded = Trace::default();
    let drawn = std::cell::Cell::new(0usize);
    let observed = std::cell::Cell::new(-1i32);

    let arm = measure_probed_at_margin(
        "splice",
        &md,
        !expanding,
        Some(top_margin),
        |rig, phase| match phase {
            Phase::BeforeToggle => {
                // Read off the LIVE view, at the moment the measurement begins, rather
                // than trusting the setter's own return: this is the reading that
                // appears in the report beside the numbers it qualifies.
                observed.set(rig.view.top_margin());
                recorded.arm(&rig.adjustment());
            }
            Phase::AfterSettle => recorded.disarm(),
        },
        |rig, folds, key| {
            drawn.set(splice_toggle(rig, folds, key, &md, Parenting::Deferred));
        },
    );

    assert_eq!(
        observed.get(),
        top_margin,
        "the live view reported a top-margin of {}px where the cell asked for \
         {top_margin}px, so this cell's numbers describe a different margin from the \
         one they are reported under",
        observed.get(),
    );
    // Only EXPANDING draws the body's children; collapsing deletes them and the region
    // render writes a summary line with no anchored widget at all.
    if expanding {
        assert!(
            drawn.get() >= children,
            "the fixture asked for {children} anchored children in the disclosure body \
             and the region render drew only {} anchored widgets, so this cell measures \
             a smaller dose than it reports",
            drawn.get(),
        );
    }
    assert!(
        arm.anchor_survived(),
        "the splice destroyed the reader's anchor at top-margin {top_margin}px / \
         {children} children, so the drift measured here describes some other line"
    );
    // Without this, a cell that recorded nothing would satisfy the identity below
    // VACUOUSLY (0 == 0 x top_margin) and be counted as confirming evidence.
    assert!(
        !recorded.emissions().is_empty(),
        "no compensating write was recorded at top-margin {top_margin}px / {children} \
         children, so this cell would confirm the identity only by measuring nothing"
    );

    Cell {
        asked: top_margin,
        observed: observed.get(),
        children,
        expanding,
        arm,
        emissions: recorded.emissions(),
    }
}

/// The shift the model predicts between one cell's deltas and the control's.
///
/// Each logged delta is claimed to be `top_margin` px LESS than the height change it
/// was compensating, so raising the margin LOWERS every delta: the prediction is
/// `-(asked - control.asked)`, and the sign is the whole content of the claim. Getting
/// it backwards makes an exact confirmation print as a mismatch, which is how a
/// measurement gets reported as its own opposite.
fn predicted_shift(cell: &Cell, control: &Cell) -> f64 {
    -f64::from(cell.observed - control.observed)
}

/// How this cell's logged deltas compare, one by one, with the control's — the model's
/// sharpest prediction, and the one drift is only a consequence of. A uniform shift
/// prints as one number and anything else as the range it actually is.
fn per_delta_shift(cell: &Cell, control: &Cell) -> String {
    let expected = predicted_shift(cell, control);
    match recorder::shift_against(&cell.emissions, &control.emissions) {
        recorder::SequenceShift::Uniform { n, shift } => format!(
            "every one of {n} shifted by exactly {shift:+.1}px (predicted \
             {expected:+.1}px){}",
            if shift == expected {
                ""
            } else {
                "  <-- MISMATCH"
            },
        ),
        recorder::SequenceShift::NotUniform { n, lo, hi } => format!(
            "{n} deltas, shifts range {lo:+.1}px .. {hi:+.1}px (predicted \
             {expected:+.1}px) <-- NOT UNIFORM",
        ),
        recorder::SequenceShift::NotComparable { ours, theirs } => {
            format!("NOT COMPARABLE: {ours} deltas against the control's {theirs}")
        }
    }
}

/// The whole grid, one direction at a time, with the control arm's cells alongside.
fn report(cells: &[Cell], expanding: bool) -> String {
    let direction = if expanding { "EXPANDING" } else { "COLLAPSING" };
    let control_margin = configured_top_margin();
    let mut out = format!(
        "\n=== top-margin knob: {direction} a disclosure ABOVE the reading position ===\n\
         \x20 model under test: drift = emissions x top_margin\n\
         \x20 {:>6} {:>9} {:>9} {:>10} {:>12} {:>12} {:>10} {:>10} {:>10}\n",
        "asked",
        "observed",
        "children",
        "emissions",
        "sum(deltas)",
        "height added",
        "drift",
        "predicted",
        "residual",
    );
    for c in cells.iter().filter(|c| c.expanding == expanding) {
        out.push_str(&format!(
            "  {:>6} {:>9} {:>9} {:>10} {:>10.1}px {:>10.1}px {:>8.1}px {:>8.1}px {:>8.1}px\n",
            c.asked,
            c.observed,
            c.children,
            c.emissions.len(),
            c.sum_of_deltas(),
            c.height_added(),
            c.drift(),
            c.predicted_drift(),
            c.drift() - c.predicted_drift(),
        ));
    }

    out.push_str("\n  the logged deltas, as runs of consecutive identical values:\n");
    for c in cells.iter().filter(|c| c.expanding == expanding) {
        out.push_str(&format!(
            "    tm={:<3} n={:<3}  {}\n",
            c.asked,
            c.children,
            recorder::runs_summary(&c.emissions),
        ));
    }

    out.push_str(&format!(
        "\n  per-delta shift against the tm={control_margin} control, same cell:\n"
    ));
    for c in cells.iter().filter(|c| c.expanding == expanding) {
        let control = at(cells, control_margin, c.children, expanding);
        out.push_str(&format!(
            "    tm={:<3} n={:<3}  {}\n",
            c.asked,
            c.children,
            per_delta_shift(c, control),
        ));
    }
    out
}

fn at(cells: &[Cell], margin: i32, children: usize, expanding: bool) -> &Cell {
    cells
        .iter()
        .find(|c| c.asked == margin && c.children == children && c.expanding == expanding)
        .expect("the grid measured this cell")
}

/// **The measurement.** The whole grid in one test, for the same reason
/// [`super::trace`]'s and [`super::wholelist`]'s are: the positive control and the cells
/// it qualifies are only meaningful against each other, and a control in a separate test
/// can be filtered out, or fail while the other passes and is read as good news.
#[gtktest::test]
fn the_drift_and_the_logged_deltas_against_the_views_top_margin() {
    if super::skip_if_gtk_compensates_top_margin("drift: top_margin identity") {
        return;
    }
    let control_margin = configured_top_margin();
    let mut cells: Vec<Cell> = Vec::new();
    for margin in margins() {
        for expanding in [true, false] {
            for children in CHILD_COUNTS {
                cells.push(cell(margin, children, expanding));
            }
        }
    }

    let out = format!("{}{}", report(&cells, true), report(&cells, false),);
    println!("{out}");

    // ── The positive control, first, and it is the whole licence for the rest. ──
    //
    // The configured-margin arm must reproduce the dose-response the other experiments
    // here record (MEASURED: drift +32 / +368 / +880 px, 2 / 23 / 55 emissions). Asserted
    // as the SHAPE, not those literals — a per-child figure is a host's font metrics, and
    // the thirty-child count is bimodal anyway (see this module's docs); the literals are
    // in the printed table for a reader to check against.
    let base = at(&cells, control_margin, 0, true);
    let loaded = at(&cells, control_margin, 10, true);
    let tripled = at(&cells, control_margin, 30, true);
    let per_child = (loaded.drift() - base.drift()) / 10.0;
    assert!(
        per_child > SLACK_PX,
        "THE RIG IS NOT EXERCISING THE PHENOMENON. At the CONFIGURED top-margin of \
         {control_margin}px, ten anchored children in the toggled body are documented \
         to drift the reader by tens of pixels each (MEASURED 33.6px), and here each \
         cost only {per_child:.1}px. Every other margin's numbers are therefore \
         meaningless — a knob turned on a rig that is not showing the effect measures \
         the rig.{out}"
    );
    assert!(
        base.drift() > 0.0 && tripled.drift() > loaded.drift(),
        "THE RIG IS NOT EXERCISING THE PHENOMENON. The control arm's drift must be \
         positive at every dose and must grow with the dose (MEASURED +32 / +368 / \
         +880 px); here it is {:.1} / {:.1} / {:.1} px.{out}",
        base.drift(),
        loaded.drift(),
        tripled.drift(),
    );
    assert!(
        loaded.emissions.len() - base.emissions.len() >= 10
            && tripled.emissions.len() - base.emissions.len() >= 30,
        "THE RIG IS NOT EXERCISING THE PHENOMENON. The control arm must show at least \
         one compensating write per region child (MEASURED 2 / 23 / 55 emissions); \
         here it showed {} / {} / {}.{out}",
        base.emissions.len(),
        loaded.emissions.len(),
        tripled.emissions.len(),
    );

    // ── The finding, half one: the identity, at every cell of the grid. ────────
    //
    // An EQUALITY with a floating-point epsilon and not a shape, unlike everything else
    // asserted on this rig — and portable BECAUSE it is a relation between whole pixels
    // rather than a font metric: change the fixture's fonts and both sides move together,
    // and it survived the bimodal count untouched. What WOULD break it is GTK itself: the
    // 4.19.3 fix named in this module's docs zeroes the drift while the count and the
    // margin stay positive, which is precisely the change a future reader needs told.
    for c in &cells {
        let residual = c.drift() - c.predicted_drift();
        assert!(
            residual.abs() <= IDENTITY_EPSILON_PX,
            "drift = emissions x top_margin does not hold at top-margin {}px / {} \
             children / {}: {} emissions at {}px predict {:.1}px and the reader drifted \
             {:.1}px, a residual of {residual:.1}px. This test records the identity as \
             EXACT in all eighteen cells, so a residual relocates the mechanism — the \
             likeliest cause is a GTK carrying the 4.19.3 fix, which zeroes the drift \
             while leaving the count and the margin positive. Re-read the tables above \
             and update this module's docs rather than widening the tolerance.{out}",
            c.asked,
            c.children,
            if c.expanding {
                "expanding"
            } else {
                "collapsing"
            },
            c.emissions.len(),
            c.observed,
            c.predicted_drift(),
            c.drift(),
        );
    }

    // ── Half two: at a zero margin the drift is GONE, not merely smaller. ─────
    //
    // Implied by the identity above and asserted separately anyway: it is the falsifiable
    // headline, the arm a "the margin is one of several terms" reading cannot survive.
    for c in cells.iter().filter(|c| c.asked == 0) {
        assert_eq!(
            c.drift(),
            0.0,
            "at a top-margin of ZERO the reader drifted {:.1}px over {} children \
             ({}), where this test records the drift as vanishing outright. A residue \
             at zero margin means a second term the margin does not explain.{out}",
            c.drift(),
            c.children,
            if c.expanding {
                "expanding"
            } else {
                "collapsing"
            },
        );
    }

    // ── Half three: EVERY logged delta moves by exactly the margin's change. ──
    //
    // The primary evidence, of which the drift is only the accumulation: each
    // compensating write is short by the margin, so raising the margin lowers every delta
    // by exactly that much. Checked only where the two sequences are comparable, the
    // count being bimodal at thirty children (see this module's docs).
    for c in &cells {
        let control = at(&cells, control_margin, c.children, c.expanding);
        let expected = predicted_shift(c, control);
        let uniform = match recorder::shift_against(&c.emissions, &control.emissions) {
            recorder::SequenceShift::Uniform { shift, .. } => shift,
            // A cell whose emission count moved with the knob has no elementwise
            // comparison to make. Reported in the table above and documented in this
            // module — a real, measured exception rather than one routed around.
            recorder::SequenceShift::NotComparable { .. } => continue,
            recorder::SequenceShift::NotUniform { .. } => f64::NAN,
        };
        assert_eq!(
            uniform,
            expected,
            "the logged deltas at top-margin {}px / {} children / {} do not sit a \
             uniform {expected:+.1}px from the control's ({}). This test records every \
             delta as shifting by exactly the margin's own change, which is the claim \
             the drift is merely the sum of.{out}",
            c.asked,
            c.children,
            if c.expanding {
                "expanding"
            } else {
                "collapsing"
            },
            per_delta_shift(c, control),
        );
    }
}
