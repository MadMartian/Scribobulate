//! **A measurement, not a feature guard** — the fourth, and the one that watches the
//! compensation happen rather than only reading what it left behind.
//!
//! # Why the vadjustment, and what it can and cannot see
//!
//! [`super::recorder`] owns the instrument and the reason its count is a LOWER BOUND on
//! `::changed` emissions. Read it before reading a count below as a refutation.
//!
//! # What it measured (GTK 4.6.9, X11/Xvfb, 700x600 pane, deferred parenting, expanding)
//!
//! Over the toggle and the whole settle that follows it, armed after every "before"
//! reading is taken and disarmed before teardown:
//!
//! | body `N` | tail `M` | emissions | sum of deltas | inserted above reader | drift |
//! |---|---|---|---|---|---|
//! | 0 | 0 | **2** | +2 128 px | +2 160 px | +32 px |
//! | 10 | 0 | **23** | +2 062 px | +2 430 px | +368 px |
//! | 0 | 10 | **2** | +2 128 px | +2 160 px | +32 px |
//! | 30 | 0 | **55** | +2 090 px | +2 970 px | +880 px |
//!
//! The sequences, as runs of consecutive identical deltas:
//!
//! * `N = 0` (both `M`): `+1874.0`, `+254.0`
//! * `N = 10`: `10 × −7.0`, `1 × −88.0`, `10 × +74.0`, `1 × +1136.0`, `1 × +344.0`
//! * `N = 30`: `30 × −7.0`, `1 × −88.0`, `22 × +74.0`, `1 × +56.0`, `1 × +704.0`
//!
//! **THE EMISSION COUNT TRACKS `N`, AND `M` ADDS NONE.** Children inside the toggled
//! region add emissions one for one — the leading run is exactly `N` long at both
//! doses — while ten children in the TAIL, sitting in the very `priv->anchored_children`
//! list the whole-list loop walks, add *not one*. Each child the region draws therefore
//! has its own `::changed` to answer for; a child the region did not touch has none.
//! Read with the lower-bound caveat above, `N` is the count this proxy sees and the
//! view's whole population is not.
//!
//! **The per-child deltas are UNIFORM, not decaying** — thirty of exactly −7.0 px, then
//! twenty-two of exactly +74.0 px, with no drift *within* either run at either dose.
//! That is the signature of one reference reused across a run rather than a reference
//! re-resolved at each step.
//!
//! Note the direction of the leading run: the offset moves **down**, once per child,
//! before any bulk compensation arrives — the reader's content sliding up — which is
//! why an endpoint-only reading of the same toggle cannot see this at all.
//!
//! **What DOES change between the two doses is the second run's LENGTH, and it
//! saturates.** The `−7.0` run is `N` at both (10, then 30); the `+74.0` run is 10 at
//! ten children and only **22** at thirty, followed by a single short `+56.0`. So
//! something bounds the up-leg after roughly twenty-odd steps while the down-leg is
//! unbounded in `N`. **What bounds it is now MEASURED** — a fixed pixel budget spent one
//! validated chunk at a time; see [`super::kink`] for the kink that budget puts in the
//! dose-response and [`super::budget`] for the budget itself.
//!
//! ⚠ **That saturation is NOT by itself the explanation of [`super::drift`]'s
//! sub-linearity, and the sign is the reason.** A *shorter* up-run is *less*
//! compensation, which would make the per-child drift larger at thirty; the measured
//! per-child drift is smaller there (33.6 px at ten against 28.3 px at thirty). The
//! trailing bulk emissions move the other way and by more (`+1136/+344` = 1 480 px at
//! ten against `+56/+704` = 760 px at thirty), and the totals do not order with `N` at
//! all: the compensation is 66 px *below* the empty-region base at ten and only 38 px
//! below it at thirty. Recorded as a structural observation for whoever takes the
//! mechanism further, not as a mechanism.
//!
//! Arithmetic worth not re-deriving, at ten: the children add 270 px of height (27 px
//! each, matching their measured heights) while the total compensation *falls* by 66 px
//! (2 128 → 2 062), so the drift excess is `270 + 66 = 336 px`, i.e. 33.6 px/child —
//! [`super::drift`]'s headline figure, and its recorded −6.6 px "shortfall" column is
//! that 66 px spread per child. The compensation does not merely miss the children's
//! pixels; it ends up worse than if they had not been there.
//!
//! # The accounting check
//!
//! `before.value + sum(deltas)` equals the value read at the end of the settle **to
//! zero**, in every cell (residual `+0.000000 px`). Nothing moved the adjustment
//! outside the emissions logged here — so this sequence is the WHOLE of what the
//! adjustment was told, and a mechanism moving `yoffset` behind the adjustment's back
//! is not what this rig is seeing. Reported because agreement is as informative as
//! disagreement: it says the search belongs in how these values are COMPUTED, not in
//! some unobserved second writer.

use super::harness::{
    measure_probed, splice_toggle, tall_document_with_body_and_tail_children, Arm, Parenting, Phase,
};
use super::recorder::{self, Emission, Trace};

/// The cells traced: the base; the loaded region (also the positive control); a loaded
/// TAIL, which is what says whether the view's whole `anchored_children` length changes
/// how many times the compensation fires — the question the drift alone cannot answer;
/// and a SECOND region dose, which is what turns "one emission per child" from a
/// coincidence at ten into a measurement, and says where [`super::drift`]'s
/// sub-linearity between ten and thirty does *not* live.
const TRACED: [(usize, usize); 4] = [(0, 0), (10, 0), (0, 10), (30, 0)];

/// The Markdown construct that renders as exactly one anchored widget — the same
/// thematic break [`super::drift`] and [`super::wholelist`] dose with.
const RULE: &str = "---";

/// Slack for the positive control, in pixels — about one text row, as elsewhere on
/// this rig.
const SLACK_PX: f64 = 12.0;

/// Tolerance for the accounting identity below, in pixels. Deliberately far tighter
/// than [`SLACK_PX`]: this is not a shape but an EQUALITY, and the only thing it has
/// to absorb is the error of summing a handful of `f64` deltas. A pixel of slack here
/// would let a real second writer hide.
const ACCOUNTING_EPSILON_PX: f64 = 1e-6;

/// One traced cell: everything [`super::harness::measure`] reads, plus the emissions.
struct Traced {
    body: usize,
    tail: usize,
    arm: Arm,
    emissions: Vec<Emission>,
}

impl Traced {
    /// What the adjustment was told to move by, in total, across every logged emission.
    fn sum_of_deltas(&self) -> f64 {
        recorder::sum_of_deltas(&self.emissions)
    }

    /// How far the adjustment actually ended up from where it started.
    fn observed_value_delta(&self) -> f64 {
        self.arm.after.value - self.arm.before.value
    }

    /// The residual of the accounting identity: what the end-of-settle value is, minus
    /// what the logged emissions say it should be. Zero means every movement of the
    /// adjustment went through a `value-changed` this trace saw.
    fn unaccounted_px(&self) -> f64 {
        self.observed_value_delta() - self.sum_of_deltas()
    }

    /// How much document the toggle inserted ABOVE the reader's marked line.
    ///
    /// Not read from the adjustment's `upper` — that is the whole document's growth,
    /// which includes nothing below the reader only by accident of this fixture.
    /// Derived from the two numbers that define the drift instead: the reader's line
    /// moved on screen by `drift`, and the viewport moved under it by
    /// `observed_value_delta`, so what went in above them is the sum. A perfect
    /// compensation would make these two equal and the drift zero.
    fn inserted_above_the_reader(&self) -> f64 {
        self.arm.content_drift_px() + self.observed_value_delta()
    }

    /// The longest run of consecutive identical deltas — [`recorder::runs`]'s finding
    /// as one number, so "uniform" can be asserted as a shape rather than as a literal
    /// sequence a host's font metrics would break.
    fn longest_uniform_run(&self) -> usize {
        recorder::longest_uniform_run(&self.emissions)
    }
}

/// Trace one cell end to end.
fn trace(body: usize, tail: usize) -> Traced {
    let md = tall_document_with_body_and_tail_children(body, tail, RULE);
    let recorder = Trace::default();
    let drawn = std::cell::Cell::new(0usize);

    let arm = measure_probed(
        "splice",
        &md,
        false,
        |rig, phase| match phase {
            Phase::BeforeToggle => recorder.arm(&rig.adjustment()),
            Phase::AfterSettle => recorder.disarm(),
        },
        |rig, folds, key| {
            drawn.set(splice_toggle(rig, folds, key, &md, Parenting::Deferred));
        },
    );

    assert!(
        drawn.get() >= body,
        "the fixture asked for {body} anchored children in the disclosure body and the \
         region render drew only {} anchored widgets, so this trace describes a smaller \
         dose than it reports",
        drawn.get(),
    );
    assert!(
        arm.anchor_survived(),
        "the splice destroyed the reader's anchor at body {body} / tail {tail}, so the \
         drift this trace is compared against describes some other line"
    );

    Traced {
        body,
        tail,
        arm,
        emissions: recorder.emissions(),
    }
}

/// The raw sequence, in order, with values — never aggregated away. Whether the deltas
/// are UNIFORM (one stale reference reused) or DECAYING (the reference re-resolved each
/// time) is only visible here, and a count plus a sum cannot express either.
fn report(traced: &Traced) -> String {
    let mut out = format!(
        "\n--- body N={} / tail M={} : {} value-changed emission(s) over the toggle and \
         its settle ---\n",
        traced.body,
        traced.tail,
        traced.emissions.len(),
    );
    out.push_str(&format!("  {:>3} {:>14} {:>14}\n", "#", "value", "delta"));
    for (i, e) in traced.emissions.iter().enumerate() {
        out.push_str(&format!(
            "  {:>3} {:>12.1}px {:>+12.1}px\n",
            i + 1,
            e.value,
            e.delta
        ));
    }
    out.push_str("\n  as runs of consecutive identical deltas:\n    ");
    out.push_str(&recorder::runs_summary(&traced.emissions));
    out.push_str(&format!(
        "\n  longest uniform run        {:>12}\n",
        traced.longest_uniform_run()
    ));

    out.push_str(&format!(
        "\n  value before settle        {:>12.1}px\n  \
         sum of logged deltas       {:>+12.1}px\n  \
         value after settle         {:>12.1}px\n  \
         before + sum               {:>12.1}px   (unaccounted {:+.6}px)\n  \
         inserted above the reader  {:>+12.1}px\n  \
         => shortfall = drift       {:>+12.1}px\n",
        traced.arm.before.value,
        traced.sum_of_deltas(),
        traced.arm.after.value,
        traced.arm.before.value + traced.sum_of_deltas(),
        traced.unaccounted_px(),
        traced.inserted_above_the_reader(),
        traced.arm.content_drift_px(),
    ));
    out
}

/// **The measurement.** Every cell in one test, for the same reason
/// [`super::wholelist`]'s grid is one: the positive control and the traces it
/// qualifies are only meaningful against each other.
#[gtktest::test]
fn each_region_child_adds_compensating_writes_and_a_tail_child_adds_none() {
    if super::skip_if_gtk_compensates_top_margin("drift: per-child compensating writes") {
        return;
    }
    let cells: Vec<Traced> = TRACED.iter().map(|&(b, t)| trace(b, t)).collect();

    let mut out = String::from(
        "\n=== per-emission trace of the compensating vadjustment writes ===\n\
         \x20 expanding a disclosure ABOVE the reading position, deferred parenting\n\
         \x20 NOTE: a ::changed that computes a ZERO compensation emits nothing \
         (GtkAdjustment\n\
         \x20 swallows a set to the value it already holds), so this count is a LOWER \
         BOUND\n\
         \x20 on ::changed emissions and a count below N is not evidence against N.\n",
    );
    for c in &cells {
        out.push_str(&report(c));
    }
    println!("{out}");

    let at = |body: usize, tail: usize| -> &Traced {
        cells
            .iter()
            .find(|c| c.body == body && c.tail == tail)
            .expect("the trace measured this cell")
    };
    let base = at(0, 0);
    let loaded = at(10, 0);
    let tailed = at(0, 10);
    let tripled = at(30, 0);

    // ── The positive control, first. ───────────────────────────────────────────
    let per_child = (loaded.arm.content_drift_px() - base.arm.content_drift_px()) / 10.0;
    assert!(
        per_child > SLACK_PX,
        "THE RIG IS NOT EXERCISING THE PHENOMENON. Ten anchored children in the toggled \
         body are documented to drift the reader by tens of pixels each (MEASURED \
         33.6px), and on this rig each cost only {per_child:.1}px. Every emission count \
         and delta above is therefore a trace of something other than the effect under \
         study.{out}"
    );

    // ── The accounting check, at every cell. ───────────────────────────────────
    //
    // Reported and asserted: agreement says the logged sequence is the WHOLE of what
    // the adjustment was told, which is what makes the emission count below mean
    // anything at all. A residual would mean something moves the offset without going
    // through `set_value`, and would move the search elsewhere entirely.
    for c in &cells {
        let unaccounted = c.unaccounted_px();
        assert!(
            unaccounted.abs() <= ACCOUNTING_EPSILON_PX,
            "the adjustment ended {unaccounted:.6}px away from where the logged \
             emissions say it should be (body N={}, tail M={}), so something moved it \
             WITHOUT going through a `value-changed` this trace could see. That is a \
             change to the finding — this test records the two as agreeing exactly — \
             and it relocates the mechanism, so re-read the trace above rather than \
             widening the tolerance.{out}",
            c.body,
            c.tail,
        );
    }

    // ── The finding, half one: the count tracks the REGION's children. ─────────
    //
    // A LOWER bound (`>= N` extra), never an equality on 23: the count is itself a
    // lower bound on `::changed` emissions — a `::changed` computing a zero
    // compensation writes nothing and is invisible here — so the direction this proxy
    // can speak to is "at least one per child", and pinning the literal would freeze a
    // host's font metrics into a finding about GTK.
    let extra = loaded.emissions.len() - base.emissions.len();
    assert!(
        extra >= 10,
        "ten anchored children in the toggled region added only {extra} compensating \
         adjustment writes to the empty region's {}. This test records at least one \
         per child (MEASURED 21 extra: two runs of ten, plus one), which is what makes \
         the emission count a term that tracks N.{out}",
        base.emissions.len(),
    );
    assert_eq!(
        tailed.emissions.len(),
        base.emissions.len(),
        "ten anchored children OUTSIDE the toggled region changed the number of \
         compensating adjustment writes ({} against {}), which would put the view's \
         whole `anchored_children` length back in play as a term — the thing \
         `wholelist`'s null result rules out from the drift side. That the two \
         populations differ HERE, in the same measurement, is the whole point of this \
         cell.{out}",
        tailed.emissions.len(),
        base.emissions.len(),
    );

    // ── The finding, half two: those writes are UNIFORM, not decaying. ─────────
    //
    // The discriminator between "one reference reused across the run" and "the
    // reference re-resolved at each step" — and the reason the sequence is printed in
    // order rather than aggregated. Asserted as a run LENGTH, which is the shape,
    // rather than as the −7.0/+74.0 the run happens to carry on this host.
    let run = loaded.longest_uniform_run();
    assert!(
        run >= 10,
        "the compensating writes for ten region children no longer come in a uniform \
         run — the longest run of identical deltas is {run}, against the ten this test \
         records (MEASURED: ten of exactly -7.0px, then ten of exactly +74.0px). A \
         DECAYING run would mean the compensation re-resolves its reference at each \
         step, which is a different mechanism: re-read the trace above and update this \
         module's docs rather than lowering the bound.{out}"
    );
    assert!(
        base.longest_uniform_run() < 10,
        "the EMPTY region already emits a uniform run of {} identical deltas, so the \
         run measured for the loaded region says nothing about its children.{out}",
        base.longest_uniform_run(),
    );

    // ── Both halves again at a second dose. ────────────────────────────────────
    //
    // "One emission per child" and "the run is uniform" are each satisfiable by
    // coincidence at a single count. Thirty children is what makes them measurements.
    // It is also where the SECOND run stops tracking N (22 of it, not 30) while the
    // first still does — see the module docs, including why that saturation cannot by
    // itself be [`super::drift`]'s sub-linearity.
    let tripled_extra = tripled.emissions.len() - base.emissions.len();
    assert!(
        tripled_extra >= 30,
        "thirty anchored children in the toggled region added only {tripled_extra} \
         compensating adjustment writes over the empty region's {}, so the \
         one-per-child reading does not survive a second dose.{out}",
        base.emissions.len(),
    );
    let tripled_run = tripled.longest_uniform_run();
    assert!(
        tripled_run >= 30,
        "the compensating writes for thirty region children come in a longest uniform \
         run of {tripled_run}, so the run tracks the child count at ten and stops \
         doing so at thirty — which would put a decay back on the table exactly where \
         the per-child drift falls off.{out}"
    );

    // ── And the drift really is a shortfall in what was written. ───────────────
    //
    // The drift is what the compensation did NOT carry, so a loaded region must show a
    // LARGER insertion compensated by a measurably SMALLER total. Without this, the
    // emission counts above are compatible with the extra writes being bookkeeping
    // that costs the reader nothing.
    assert!(
        loaded.sum_of_deltas() < base.sum_of_deltas() - SLACK_PX,
        "the loaded region's compensation ({:.1}px) did not fall short of the empty \
         region's ({:.1}px), so the per-child drift is not a shortfall in the \
         adjustment write at all and this trace is pointed at the wrong quantity.{out}",
        loaded.sum_of_deltas(),
        base.sum_of_deltas(),
    );
}
