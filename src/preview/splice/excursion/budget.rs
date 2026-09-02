//! **A measurement, not a feature guard** — the seventh, and it tests the same
//! validation budget [`super::kink`] does, from the other side.
//!
//! [`super::kink`] moves the DOSE and finds the kink the budget produces at one chunk
//! height. This moves the chunk HEIGHT and asks how long one validation pass runs, which
//! is the budget's own arithmetic rather than a consequence of it. The two can disagree:
//! a saturation that is a constant 22 rather than a quotient passes the kink grid and
//! fails here, and that is the whole reason this file exists.
//!
//! Separate file from [`super::kink`] because it is a different question over the same
//! plumbing — and because adding it there crossed the 500-line soft limit
//! (POLICY § Code style), which is where the cut was owed anyway.
//!
//! # The loop this arm is written against: `ceil`, not `floor`
//!
//! `gtk_text_layout_validate` (`gtktextlayout.c:1033-1051`, 4.6.9) spends the budget as
//!
//! ```text
//! while (max_pixels > 0 && validate (...)) { max_pixels -= new_height; emit ::changed; }
//! ```
//!
//! **The guard is tested BEFORE the decrement**, so a pass validating chunks of height
//! `h` against a budget `B` emits `ceil(B / h)` of them — the chunk that takes the
//! budget past zero is emitted anyway. Stated as the general rule the pass length obeys:
//!
//! ```text
//! sum of the first L-1 chunk heights  <  B  <=  sum of the first L
//! ```
//!
//! This file was previously written against `floor(B / h)`, which is one short. `floor`
//! and `ceil` coincide at two of the three chunk heights below by arithmetic accident, so
//! the third arm read as *the documented 2 000 does not survive* rather than as *the
//! formula is wrong by one*. Everything the old framing then derived from it — an implied
//! budget bracket 16 px above the literal, and an "open question" about where that pixel
//! came from — was an artefact of the missing chunk.
//!
//! # ⚠ THE UP-LEG IS A RUN LENGTH, AND A RUN LENGTH IS NOT ALWAYS A PASS LENGTH
//!
//! [`super::dose::UpLeg`] carries the full account. In short: consecutive passes charging
//! the SAME delta merge into one run of the encoding with no separator, so the extractor
//! cannot recover the boundary. MEASURED at the half filler, three runs of one arm:
//! `56x+56.0` once and `28x+56.0` twice — one 28-chunk pass, run twice and run once. What
//! separates two passes is a pass whose terminal chunk is SHORTER than the rest; that
//! emission is its own run and nothing merges across it.
//!
//! So each arm is graded on what its own trace establishes, never on a number assumed to
//! be a pass length, and an arm that establishes no boundary contributes a divisibility
//! check and **no bracket at all** rather than a bracket derived from a multiple.
//!
//! # What it measured
//!
//! MEASURED (GTK 4.6.9, X11/Xvfb, 700x600 pane, `N = 60`), three body fillers, three runs
//! each, in BOTH ambient-metric regimes — this test run ALONE, and the same test inside
//! the full lib suite, which gives every filler a different height (see the last section):
//!
//! | run | filler | chunk | uniform run | terminal chunk | `ceil(2000/chunk)` | budget bracket |
//! |---|---|---|---|---|---|---|
//! | alone | half | 72 px | 28, or 56 as two merged passes | none | 28 | not established |
//! | alone | standing | 90 px | 22 | 72 px | 23 | `(1980, 2052]` |
//! | alone | double | 127 px | 15 | 109 px | 16 | `(1905, 2014]` |
//! | suite | half | 60 px | 33 | 45 px | 34 | `(1980, 2025]` |
//! | suite | standing | 74 px | 27 | 15 px | 28 | `(1998, 2013]` |
//! | suite | double | 89 px | 22 | 74 px | 23 | `(1958, 2032]` |
//!
//! Every arm's observed run is `ceil(2000 / chunk)`, less one where a shorter terminal
//! chunk closes the pass, and an exact multiple of that where none does — at **six**
//! chunk heights spanning 60 to 127 px. The arms that establish a boundary intersect at
//! `(1980, 2014]` alone and at `(1998, 2013]` under the suite, and **both contain the
//! literal `gtk_text_layout_validate (layout, 2000)` of `gtktextview.c:4817`.**
//!
//! **The literal 2 000 fits.** No pixel is unaccounted for and there is no open question
//! about one; the earlier claim that the budget sits 16 px above the constant was the
//! `floor` framing, not a measurement.
//!
//! The suite's `standing` arm is worth reading twice: its terminal chunk is **15 px**,
//! logged as `1x-1.0` because a chunk shorter than the 16 px margin moves the adjustment
//! BACKWARDS. It is a chunk, it closes the pass, and `27 * 74 + 15 = 2013` is the
//! tightest upper bound any arm here produces — see [`super::dose::UpLeg`] for why the
//! reading has to be in chunk heights rather than in deltas.
//!
//! # The up-leg RESUMES, and that is where the bimodality comes from
//!
//! At `N = 60` every arm's sequence shows the pass stopping and another STARTING at the
//! same delta — `22x+74, 1x+56, 1x+2, 22x+74` at the standing filler, two passes of 15 at
//! the double one, and the number of passes varying run to run at a fixed filler. Each
//! pass is one `incremental_validate_callback` spending a fresh budget. That is the
//! mechanism [`super::margin`] recorded as an open question: the count is bimodal because
//! the number of passes that complete before the settling bulk write is not
//! deterministic, and [`super::kink`] measures the two modes it produces.
//!
//! It is also exactly the mechanism that merges two runs when no terminal chunk falls
//! between them, which is why the half arm's reading is bimodal in the *encoding* while
//! the standing and double arms' never are.
//!
//! The `standing` arm is the POSITIVE CONTROL: its 22 is the saturation
//! [`super::trace`], [`super::margin`] and [`super::kink`] all measure, reproduced here
//! at a different dose and in the same run as the two arms it qualifies.
//!
//! # ⚠ THE CHUNK HEIGHTS MOVE WITH THE AMBIENT FONT METRICS
//!
//! As in [`super::kink`], running inside the full lib suite rather than alone gives the
//! same fillers different heights — 60 / 74 / 89 px against 72 / 90 / 127 px. Every
//! assertion below is therefore written against the height each run MEASURES; the table
//! above is a record, never a literal to compare against. Having both regimes is what
//! turns three chunk heights into six, and the order-dependence into a second sample.

/// The arithmetic above, driven from the RECORDED encodings instead of a live window —
/// its own file because it would have pushed this one past the 500-line soft limit.
mod derivation;

use super::dose::{run_with_filler, UpLeg};
use super::harness::FILLER;
use super::margin::configured_top_margin;
use super::recorder::{self, Emission};

/// The dose for the line-height knob. Above every saturation index the three fillers
/// produce, so each arm's up-leg is bounded by the BUDGET rather than by the dose —
/// which is the whole point of moving the height rather than the count.
const HEIGHT_KNOB_DOSE: usize = 60;

/// How many times each filler is run. Fewer than [`super::kink`]'s: the pass LENGTH is
/// the reading here, and the bimodality that file documents appears PAST the up-leg, in
/// how the remainder is carried — so it moves the total count and not this run.
const HEIGHT_KNOB_REPEATS: usize = 3;

/// The pixel budget `incremental_validate_callback` passes to `gtk_text_layout_validate`
/// (`gtktextview.c:4817`, 4.6.9) — the literal this arm tests DIRECTLY rather than
/// through the kink.
const DOCUMENTED_BUDGET_PX: f64 = 2000.0;

/// One arm's reading.
struct Arm {
    filler: &'static str,
    /// Every emission of the toggle, kept for the runs summary the table prints.
    emissions: Vec<Emission>,
    /// The up-leg, boundary status and all — never a bare length. Its
    /// [`UpLeg::chunk`] is the height of one validated chunk.
    leg: UpLeg,
}

impl Arm {
    /// How many chunks a pass emits at this chunk height under the documented budget:
    /// `ceil(B / h)`, because the loop's guard is tested before its decrement.
    fn pass_length(&self) -> usize {
        (DOCUMENTED_BUDGET_PX / self.leg.chunk).ceil() as usize
    }

    /// The UNIFORM PREFIX that pass length predicts for the encoding: the whole pass
    /// where every chunk is the same height, and one less where a shorter terminal chunk
    /// closes it (that chunk being a run of its own).
    fn predicted_prefix(&self) -> usize {
        self.pass_length() - usize::from(self.leg.terminal.is_some())
    }

    /// How many passes the observed run accounts for, under the prediction — `None` when
    /// the run is not a whole multiple of it, which is the model failing rather than a
    /// number to round.
    fn passes(&self) -> Option<usize> {
        let prefix = self.predicted_prefix();
        (prefix > 0 && self.leg.run.is_multiple_of(prefix)).then(|| self.leg.run / prefix)
    }

    /// The OBSERVED uniform prefix of ONE pass: the run divided by however many passes it
    /// holds. Equal to the run itself wherever a terminal chunk established the boundary,
    /// and the run divided down wherever the prediction accounted for a merge.
    ///
    /// `None` exactly when [`Arm::passes`] is — a caller must not get a number back from
    /// a run the model could not decompose.
    fn observed_prefix(&self) -> Option<usize> {
        self.passes().map(|k| self.leg.run / k)
    }

    /// The half-open bracket of budgets consistent with this arm, **only where the trace
    /// establishes a pass boundary**.
    ///
    /// From the loop rule: the pass's first `L-1` chunks did not exhaust the budget and
    /// its `L` chunks did. With a terminal chunk of height `s` closing a prefix of `p`
    /// chunks of height `h`, that is `p*h < B <= p*h + s` — derived from heights this arm
    /// MEASURED and not from the documented constant, which is what makes it a constraint
    /// the constant can be checked against rather than a restatement of it.
    ///
    /// `None` where no terminal chunk fell: the run is then `k` passes for an
    /// unrecoverable `k`, and a bracket built on it would be arithmetic over a quantity
    /// the instrument cannot read.
    fn bracket(&self) -> Option<(f64, f64)> {
        let terminal = self.leg.terminal?;
        let lo = self.leg.run as f64 * self.leg.chunk;
        Some((lo, lo + terminal))
    }
}

/// The body fillers: about half the standing one, the standing one, and twice it.
///
/// Halving and doubling rather than two arbitrary lengths, because a quotient predicts a
/// pass length that roughly doubles when the chunk halves and halves when it doubles —
/// two predictions in opposite directions that a constant cannot satisfy at once.
/// Wrapping is not linear in character count, so the achieved chunk height is MEASURED
/// off the deltas and reported, never assumed.
fn body_fillers() -> [(&'static str, String); 3] {
    let half: String = FILLER.chars().take(FILLER.chars().count() / 2).collect();
    [
        ("half", half),
        ("standing", FILLER.to_string()),
        ("double", format!("{FILLER} {FILLER}")),
    ]
}

/// The per-arm table: what each trace showed and what the model predicts of it.
fn arm_report(arms: &[Arm], top_margin: f64) -> String {
    let mut out = format!(
        "\n=== line-height knob: how long one validation pass runs, at N={HEIGHT_KNOB_DOSE} \
         ===\n\
         \x20 chunk height = the logged shift + the {top_margin:.0}px each write is short \
         by\n\
         \x20 a `?` on the run means NO terminal chunk closed it, so the encoding cannot \
         say\n\x20 how many passes it holds — see `dose::UpLeg`\n\
         \x20 {:>10} {:>10} {:>13} {:>10} {:>10} {:>8} {:>10} {:>22}\n",
        "filler",
        "emissions",
        "chunk height",
        "uniform run",
        "terminal",
        "ceil",
        "passes",
        "budget bracket",
    );
    for a in arms {
        out.push_str(&format!(
            "  {:>10} {:>10} {:>11.0}px {:>10} {:>10} {:>8} {:>10} {}\n",
            a.filler,
            a.emissions.len(),
            a.leg.chunk,
            a.leg.column(),
            a.leg
                .terminal
                .map_or_else(|| "none".to_string(), |t| format!("{t:.0}px")),
            a.pass_length(),
            a.passes()
                .map_or_else(|| "NOT A MULTIPLE".to_string(), |k| format!("{k}")),
            a.bracket().map_or_else(
                || "     not established".to_string(),
                |(lo, hi)| format!("{lo:>10.0}..{hi:<10.0}"),
            ),
        ));
    }
    out.push_str("\n  the logged deltas, as runs of consecutive identical values:\n");
    for a in arms {
        out.push_str(&format!(
            "    {:<10} {}\n",
            a.filler,
            recorder::runs_summary(&a.emissions),
        ));
    }
    out
}

/// **The second measurement.** How long does one validation pass run, as the chunk height
/// moves?
///
/// What is ASSERTED is the shape this rig can see: that the knob moved the chunk height
/// at all, that the pass shortens as the chunk grows, and that every arm's observed run
/// is a whole number of passes of the length `ceil(2000 / chunk)` predicts. The BUDGET's
/// value is reported rather than asserted — a bracket is a constraint, and pinning it
/// would freeze this host's font metrics into a finding about GTK.
#[gtktest::test]
fn the_up_legs_length_against_the_bodys_chunk_height() {
    if super::skip_if_gtk_compensates_top_margin("drift: validation-budget quotient") {
        return;
    }
    let top_margin = f64::from(configured_top_margin());
    let mut arms: Vec<Arm> = Vec::new();
    for (filler, text) in body_fillers() {
        for _ in 0..HEIGHT_KNOB_REPEATS {
            let run = run_with_filler(HEIGHT_KNOB_DOSE, &text);
            arms.push(Arm {
                filler,
                emissions: run.emissions().to_vec(),
                leg: run.up_leg(top_margin),
            });
        }
    }

    let out = arm_report(&arms, top_margin);
    println!("{out}");

    // ── The knob must have taken, or this measures one height three times. ─────
    let chunk_of = |filler: &str| -> f64 {
        arms.iter()
            .find(|a| a.filler == filler)
            .map(|a| a.leg.chunk)
            .expect("every arm was measured")
    };
    let (half, standing, double) = (chunk_of("half"), chunk_of("standing"), chunk_of("double"));
    assert!(
        half < standing && standing < double,
        "the filler knob did not move the chunk height ({half:.0} / {standing:.0} / \
         {double:.0}px), so the three arms measure one height under three names and \
         nothing below is a knob experiment.{out}"
    );

    // ── The instrument, before anything it reads is interpreted. ───────────────
    //
    // Every arm's observed run must be a whole number of the passes the model predicts.
    // This is what the old `floor` framing failed and what the merged-run reading hid:
    // 56 is not a multiple of 27, and reading it as a pass length fed 56 into arithmetic
    // built for 28. A non-multiple here is the model being wrong, and it is graded before
    // any budget is derived, because a derivation from a quantity the instrument cannot
    // read is worse than no derivation.
    for a in &arms {
        let prefix = a.predicted_prefix();
        assert!(
            a.passes().is_some_and(|k| k >= 1),
            "the {} arm's uniform run of {} does not divide by the {prefix} chunks a \
             {DOCUMENTED_BUDGET_PX:.0}px budget predicts at a {:.0}px chunk \
             (ceil = {}, less one for the terminal chunk where there is one). A pass \
             length that does not divide the run is the BUDGET MODEL failing, not a \
             tolerance to widen — re-read the sequences above and update this module's \
             docs.{out}",
            a.filler,
            a.leg.run,
            a.leg.chunk,
            a.pass_length(),
        );
    }

    // ── The positive control: the standing arm still saturates BELOW the dose. ─
    //
    // Not the literal 22 — a chunk height is a host's font metrics — but the fact that
    // the arm this whole directory's tables were taken on is bounded by the budget at
    // all. An arm running the full 60 would mean the budget never bit and every reading
    // here describes a dose limit instead.
    let prefix_of = |filler: &str| -> usize {
        arms.iter()
            .find(|a| a.filler == filler)
            .and_then(Arm::observed_prefix)
            .expect("every arm's run divides by its predicted prefix, asserted above")
    };
    let standing_prefix = prefix_of("standing");
    assert!(
        standing_prefix < HEIGHT_KNOB_DOSE,
        "THE RIG IS NOT EXERCISING THE PHENOMENON. At the standing filler one pass spans \
         {standing_prefix} chunks against a dose of {HEIGHT_KNOB_DOSE}, so the up-leg was \
         bounded by the DOSE and not by the budget — which is the only thing this file \
         measures.{out}"
    );

    // ── The finding, half one: the pass shortens as the chunk grows. ───────────
    //
    // The direction a quotient predicts and a constant saturation does not, which is the
    // discriminator the kink grid on its own cannot supply. Taken over the OBSERVED
    // per-pass prefix rather than the raw run, because a merged run is `k` passes and
    // would compare a doubled number against a single one — the divisibility assertion
    // above is what licenses dividing it down, and is why that one is graded first.
    for pair in [("half", "standing"), ("standing", "double")] {
        let (shorter, taller) = pair;
        assert!(
            prefix_of(shorter) > prefix_of(taller),
            "the pass did not shorten when the chunk height rose ({shorter} {} chunks at \
             {:.0}px against {taller} {} at {:.0}px), which is the direction a pixel \
             budget spent per chunk predicts and a constant saturation does not. \
             DERIVE the reading before drawing any conclusion from it: these are \
             per-pass prefixes, and a prefix is the observed run divided by a merge \
             factor the check above supplied. Read the sequences, establish what each \
             arm's trace actually shows, and update this module's docs — do not widen \
             this into a tolerance and do not report a verdict about GTK from it.{out}",
            prefix_of(shorter),
            chunk_of(shorter),
            prefix_of(taller),
            chunk_of(taller),
        );
    }

    // ── Half two: the budget the bracketing arms agree on. ────────────────────
    //
    // REPORTED, not asserted against the documented literal. Only arms whose trace
    // establishes a pass boundary contribute; the rest say so and contribute nothing,
    // because a bracket over a run that may be two merged passes is arithmetic over a
    // quantity the instrument cannot read. That is precisely the step the previous
    // version of this file got wrong.
    let brackets: Vec<(&str, f64, f64)> = arms
        .iter()
        .filter_map(|a| a.bracket().map(|(lo, hi)| (a.filler, lo, hi)))
        .collect();
    assert!(
        !brackets.is_empty(),
        "INSTRUMENT ERROR: no arm's trace closed its up-leg with a shorter terminal \
         chunk, so not one pass boundary was established and this run can bracket the \
         budget from nothing. That is the instrument declining to answer, not a finding \
         about GTK.{out}"
    );
    let lo = brackets
        .iter()
        .map(|&(_, lo, _)| lo)
        .fold(f64::MIN, f64::max);
    let hi = brackets
        .iter()
        .map(|&(_, _, hi)| hi)
        .fold(f64::MAX, f64::min);

    // The instrument must agree with itself BEFORE its output is interpreted. An empty
    // interval is not evidence against the budget model; it is this rig reporting that
    // its own inputs are mutually inconsistent, and the only honest response is to fail
    // as an instrument error rather than to publish a verdict about GTK. The version of
    // this file that printed `[4032, 2032)` and concluded the model was "refuted
    // outright" is the failure this guard exists to make impossible.
    assert!(
        lo < hi,
        "INSTRUMENT ERROR: this rig disagrees with itself. The per-arm brackets do not \
         intersect — ({lo:.0}, {hi:.0}] is empty — and they were derived from these \
         arms: {brackets:?}. An empty interval says the derivation's inputs are mutually \
         inconsistent, so NOTHING may be concluded from it about the validation budget. \
         Re-read the sequences above and fix the derivation before reading any verdict \
         into it.{out}"
    );
    println!(
        "\n  budgets consistent with every arm that established a pass boundary: \
         ({lo:.0}, {hi:.0}]\n  the documented {DOCUMENTED_BUDGET_PX:.0} is {}\n  \
         (arms with no terminal chunk establish no boundary and are excluded: {})\n",
        if lo < DOCUMENTED_BUDGET_PX && DOCUMENTED_BUDGET_PX <= hi {
            "INSIDE it"
        } else {
            "OUTSIDE it"
        },
        arms.len() - brackets.len(),
    );
}
