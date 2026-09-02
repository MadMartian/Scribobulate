//! One run of one dose, and the distribution a repeated dose produces.
//!
//! Split out when a SECOND experiment needed the same apparatus — [`super::kink`] turns
//! the dose knob and [`super::budget`] the chunk-height one, and both ask the same
//! question of the same sequence. The cut is by cause rather than by size, exactly as
//! [`super::recorder`]'s was: this file owns **driving one toggle at a dose and reading
//! its emission sequence**, and its callers own **what the reading is compared
//! against**. Two experiments measuring through two copies of this would be free to
//! drift apart, and their numbers being comparable is the whole point of the second one.
//!
//! The bimodality [`super::kink`] documents is why [`modes`] exists at all: a dose has a
//! DISTRIBUTION of emission counts, not a value, and every reading here is reported
//! against one mode of it rather than against an average that never occurred.

use super::harness::{
    measure_probed, splice_toggle, tall_document_with_body_filler, Arm, Parenting, Phase, FILLER,
};
use super::recorder::{self, Emission, Trace};

/// The Markdown construct that renders as exactly one anchored widget — the thematic
/// break every other experiment in this directory doses with, so the counts are
/// comparable with theirs.
pub(super) const RULE: &str = "---";

/// One run of one dose.
pub(super) struct Run {
    pub(super) dose: usize,
    arm: Arm,
    emissions: Vec<Emission>,
}

/// The second, longer uniform run of the emission sequence — the "up-leg" the budget is
/// claimed to cut short — **and whether the encoding can tell where the pass that
/// produced it ended**.
///
/// # ⚠ A RUN LENGTH IS NOT A PASS LENGTH, and the difference is not always recoverable
///
/// [`recorder::runs`] collapses CONSECUTIVE EQUAL deltas. A validation pass charges the
/// same delta as the pass before it whenever both validate chunks of the same height, so
/// two such passes land in **one** run with nothing between them to mark the boundary —
/// and no reading of the run-length encoding can recover where the first ended. MEASURED
/// at the half filler (chunk 72 px), three runs of one arm: `56x+56.0` once and
/// `28x+56.0` twice, which are one 28-chunk pass run twice and run once. Reading the 56
/// as a pass length is what this type exists to stop; it produced a confident, wrong
/// arithmetic result and a false conclusion about GTK downstream of it.
///
/// What DOES separate two passes is a pass whose TERMINAL chunk is shorter than the rest:
/// that emission is a run of its own, and no following pass can merge across it. So the
/// boundary is established exactly when [`UpLeg::terminal`] is `Some`, which is read off
/// the trace rather than assumed.
///
/// # The reading is in CHUNK HEIGHTS, never in deltas
///
/// Every height here is the logged delta **plus** the `top_margin` each write on this GTK
/// is short by ([`super::margin`]'s identity), and the conversion is not cosmetic. A chunk
/// shorter than the margin logs a NEGATIVE delta: MEASURED under the full lib suite, a
/// 74 px chunk's pass closes with `1x-1.0`, which is a 15 px chunk against a 16 px margin.
/// A terminal test written on the delta's sign misses it, reports the boundary as
/// unestablished, and — correctly, but uselessly — declines to answer. Ask whether the
/// CHUNK is shorter, which is the question the loop is actually about.
pub(super) struct UpLeg {
    /// The observed run of consecutive identical deltas: one pass's prefix when
    /// [`UpLeg::terminal`] is `Some`, and `k` of them for some unrecoverable `k >= 1`
    /// when it is `None`.
    pub(super) run: usize,
    /// The height of each chunk in that run.
    pub(super) chunk: f64,
    /// The height of the single shorter chunk that closes the pass, where there is one.
    pub(super) terminal: Option<f64>,
}

impl UpLeg {
    /// The per-pass prefix where the trace establishes one, and `None` where it does not
    /// — the whole point being that a caller must handle the second case rather than
    /// receive a number that might silently be a multiple.
    pub(super) fn established_prefix(&self) -> Option<usize> {
        self.terminal.map(|_| self.run)
    }

    /// The column as it may honestly be printed: a length where a boundary was
    /// established, and the run marked UNRESOLVED where it was not.
    pub(super) fn column(&self) -> String {
        match self.terminal {
            Some(_) => format!("{}", self.run),
            None => format!("{}?", self.run),
        }
    }
}

/// Read the up-leg out of one emission sequence, against the margin every write is short
/// by.
///
/// The run is located as the first multi-emission run AFTER the leading per-child one
/// rather than by index, so a fixture that changes how many single-emission writes
/// separate the two does not silently make this read a different run. The terminal chunk
/// is the emission immediately after it, taken only when it is a run of ONE whose CHUNK
/// HEIGHT lies strictly between zero and the run's — which is what a pass's last, shorter
/// chunk looks like, and what a fresh bulk write (an order of magnitude taller) does not.
pub(super) fn up_leg(emissions: &[Emission], top_margin: f64) -> UpLeg {
    let runs = recorder::runs(emissions);
    let Some(at) = runs.iter().skip(1).position(|(n, _)| *n > 1).map(|i| i + 1) else {
        return UpLeg {
            run: 0,
            chunk: 0.0,
            terminal: None,
        };
    };
    let (run, delta) = runs[at];
    let chunk = delta + top_margin;
    let terminal = runs.get(at + 1).and_then(|&(n, next)| {
        let height = next + top_margin;
        (n == 1 && chunk > 0.0 && height > 0.0 && height < chunk).then_some(height)
    });
    UpLeg {
        run,
        chunk,
        terminal,
    }
}

impl Run {
    pub(super) fn count(&self) -> usize {
        self.emissions.len()
    }

    /// Every logged emission, for a caller asking a different question of the same
    /// sequence — [`super::budget`] reads the up-leg out of it.
    pub(super) fn emissions(&self) -> &[Emission] {
        &self.emissions
    }

    /// The up-leg — the run the budget is claimed to cut short, and the term the whole
    /// formula turns on. Read off the sequence rather than inferred from the total, and
    /// returned as an [`UpLeg`] rather than a length because the encoding does not always
    /// establish where the pass ended; see that type.
    pub(super) fn up_leg(&self, top_margin: f64) -> UpLeg {
        up_leg(&self.emissions, top_margin)
    }

    pub(super) fn drift(&self) -> f64 {
        self.arm.content_drift_px()
    }

    /// The margin the identity is checked against, read off the live view's configured
    /// value rather than restated — see [`super::margin::configured_top_margin`]'s own
    /// reasoning about a second copy.
    pub(super) fn predicted_drift(&self, top_margin: f64) -> f64 {
        self.count() as f64 * top_margin
    }
}

/// One dose's distinct emission counts, ascending, each with how many runs produced it.
pub(super) fn modes(runs: &[Run], dose: usize) -> Vec<(usize, usize)> {
    let mut counts: Vec<usize> = runs
        .iter()
        .filter(|r| r.dose == dose)
        .map(Run::count)
        .collect();
    counts.sort_unstable();
    let mut out: Vec<(usize, usize)> = Vec::new();
    for c in counts {
        match out.last_mut() {
            Some((count, n)) if *count == c => *n += 1,
            _ => out.push((c, 1)),
        }
    }
    out
}

/// The up-leg observed for one dose at one of its modes — the term the formula turns on,
/// read straight off the sequence, boundary status and all.
pub(super) fn up_leg_at(runs: &[Run], dose: usize, count: usize, top_margin: f64) -> UpLeg {
    runs.iter()
        .find(|r| r.dose == dose && r.count() == count)
        .map(|r| r.up_leg(top_margin))
        .expect("the mode came from a run of this dose")
}

/// The drift observed for one dose at one of its modes — every run at that count agreed
/// on the drift, the identity below being exact, so the first is the reading.
pub(super) fn drift_at(runs: &[Run], dose: usize, count: usize) -> f64 {
    runs.iter()
        .find(|r| r.dose == dose && r.count() == count)
        .map(Run::drift)
        .expect("the mode came from a run of this dose")
}

/// Measure one run of one dose: build the fixture, park the reader, toggle through the
/// splice, and record every `value-changed` across the settle.
pub(super) fn run(dose: usize) -> Run {
    run_with_filler(dose, FILLER)
}

/// [`run`], with the body's paragraph filler — and so the height of a validated chunk —
/// chosen by the caller. See [`the_up_legs_length_against_the_bodys_chunk_height`].
pub(super) fn run_with_filler(dose: usize, filler: &str) -> Run {
    let md = tall_document_with_body_filler(dose, 0, RULE, filler);
    let recorded = Trace::default();
    let drawn = std::cell::Cell::new(0usize);

    let arm = measure_probed(
        "splice",
        &md,
        false,
        |rig, phase| match phase {
            Phase::BeforeToggle => recorded.arm(&rig.adjustment()),
            Phase::AfterSettle => recorded.disarm(),
        },
        |rig, folds, key| {
            drawn.set(splice_toggle(rig, folds, key, &md, Parenting::Deferred));
        },
    );

    assert!(
        drawn.get() >= dose,
        "the fixture asked for {dose} anchored children in the disclosure body and the \
         region render drew only {} anchored widgets, so this run measures a smaller \
         dose than it reports",
        drawn.get(),
    );
    assert!(
        arm.anchor_survived(),
        "the splice destroyed the reader's anchor at {dose} children, so the drift \
         measured here describes some other line"
    );
    assert!(
        !recorded.emissions().is_empty(),
        "no compensating write was recorded at {dose} children, so this run would \
         confirm the identity below only by measuring nothing"
    );

    Run {
        dose,
        arm,
        emissions: recorded.emissions(),
    }
}

/// The extractor's own guard, over the RECORDED encodings rather than a fresh window.
///
/// The merged reading is INTERMITTENT on a live rig — it appeared once in three runs of
/// one arm and then in none of nine more — so waiting for it is waiting on a coin flip,
/// and a coin flip is not a regression guard. These bodies replay the exact run-length
/// encodings that were measured, which makes the merge deterministic and pins this
/// extractor on the case the previous one got wrong.
mod extractor {
    use super::super::recorder::Emission;
    use super::up_leg;

    /// The margin every write on this GTK is short by, at the rig's configured value —
    /// a fixture constant here rather than a live reading, because these bodies replay
    /// encodings that were recorded at exactly it.
    const TOP_MARGIN_PX: f64 = 16.0;

    /// Expand `(count, delta)` runs into an emission sequence. The values are cumulative
    /// and unused by the extractor, which reads deltas only; they are filled in so the
    /// fixture is a real sequence rather than a shape with holes in it.
    fn sequence(runs: &[(usize, f64)]) -> Vec<Emission> {
        let mut value = 0.0;
        let mut out = Vec::new();
        for &(count, delta) in runs {
            for _ in 0..count {
                value += delta;
                out.push(Emission { value, delta });
            }
        }
        out
    }

    /// MEASURED at the half filler (chunk 72 px), the encoding that merges: two 28-chunk
    /// passes charging the same delta land in one run of 56 with nothing between them.
    /// The extractor must DECLINE to call that a pass boundary — reading it as one is
    /// what fed 56 into arithmetic built for 28.
    #[gtktest::test]
    fn two_passes_charging_one_delta_establish_no_boundary() {
        let merged = up_leg(
            &sequence(&[(60, -7.0), (1, -88.0), (56, 56.0), (1, 344.0)]),
            TOP_MARGIN_PX,
        );
        assert_eq!(merged.run, 56, "the uniform run itself is read as measured");
        assert_eq!(merged.chunk, 72.0);
        assert_eq!(
            merged.terminal, None,
            "nothing shorter closes the run, so no boundary is established"
        );
        assert_eq!(merged.established_prefix(), None);
        assert_eq!(merged.column(), "56?");

        // The same arm at the same chunk height, ONE pass. The two encodings differ only
        // in the run's length, which is the whole point: nothing in either says how many
        // passes it holds.
        let single = up_leg(
            &sequence(&[(60, -7.0), (1, -88.0), (28, 56.0), (1, 1514.0), (1, 830.0)]),
            TOP_MARGIN_PX,
        );
        assert_eq!(single.run, 28);
        assert_eq!(single.established_prefix(), None);
    }

    /// MEASURED at the standing (90 px) and double (127 px) fillers: the pass's terminal
    /// chunk is shorter than the rest, so it is a run of its own and nothing merges
    /// across it. That single emission is the whole of what establishes a boundary.
    #[gtktest::test]
    fn a_shorter_terminal_chunk_establishes_the_boundary() {
        let standing = up_leg(
            &sequence(&[
                (60, -7.0),
                (1, -88.0),
                (22, 74.0),
                (1, 56.0),
                (1, 2.0),
                (22, 74.0),
            ]),
            TOP_MARGIN_PX,
        );
        assert_eq!(standing.run, 22);
        assert_eq!(standing.chunk, 90.0);
        assert_eq!(
            standing.terminal,
            Some(72.0),
            "the 56px write is a 72px chunk"
        );
        assert_eq!(standing.established_prefix(), Some(22));
        assert_eq!(standing.column(), "22");

        let double = up_leg(
            &sequence(&[(60, -7.0), (1, -88.0), (15, 111.0), (1, 93.0), (1, 1762.0)]),
            TOP_MARGIN_PX,
        );
        assert_eq!(double.established_prefix(), Some(15));
    }

    /// **The regression this extractor was rewritten for.** MEASURED inside the full lib
    /// suite, where the ambient font metrics give a 74 px chunk: the pass closes with
    /// `1x-1.0`, a 15 px chunk logged as a NEGATIVE delta because it is shorter than the
    /// margin. A terminal test on the delta's sign misses it, and the whole grid then
    /// reports that no boundary was established anywhere — the instrument declining to
    /// answer a question it can answer. The test belongs in chunk-height space.
    #[gtktest::test]
    fn a_chunk_shorter_than_the_margin_still_closes_the_pass() {
        let standing = up_leg(
            &sequence(&[
                (60, -7.0),
                (1, -61.0),
                (27, 58.0),
                (1, -1.0),
                (1, 1523.0),
                (1, 917.0),
            ]),
            TOP_MARGIN_PX,
        );
        assert_eq!(standing.run, 27);
        assert_eq!(standing.chunk, 74.0);
        assert_eq!(
            standing.terminal,
            Some(15.0),
            "a -1.0px write against a 16px margin is a 15px chunk, and 15 < 74"
        );
        assert_eq!(standing.established_prefix(), Some(27));
    }

    /// A BULK write after the run is not a terminal chunk, and neither is nothing at all.
    /// These are the shapes that would let a boundary be claimed where none exists; the
    /// discriminator is that a terminal chunk is SHORTER than the chunks it closes.
    #[gtktest::test]
    fn a_bulk_write_after_the_run_is_not_a_terminal_chunk() {
        let bulk = up_leg(
            &sequence(&[(60, -7.0), (1, -88.0), (28, 56.0), (1, 1514.0)]),
            TOP_MARGIN_PX,
        );
        assert_eq!(bulk.terminal, None, "a 1530px chunk is not a shorter one");

        let nothing_after = up_leg(
            &sequence(&[(60, -7.0), (1, -88.0), (28, 56.0)]),
            TOP_MARGIN_PX,
        );
        assert_eq!(nothing_after.terminal, None);

        // A write whose implied chunk height is at or below zero is not a chunk at all.
        let collapsed = up_leg(
            &sequence(&[(60, -7.0), (1, -88.0), (28, 56.0), (1, -40.0)]),
            TOP_MARGIN_PX,
        );
        assert_eq!(collapsed.terminal, None, "-24px is not a chunk height");

        let empty_region = up_leg(&sequence(&[(1, 1874.0), (1, 254.0)]), TOP_MARGIN_PX);
        assert_eq!(
            empty_region.run, 0,
            "the empty region has no multi-emission run to read"
        );
        assert_eq!(empty_region.established_prefix(), None);
    }
}
