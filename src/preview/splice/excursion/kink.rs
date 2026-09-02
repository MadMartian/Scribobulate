//! **A measurement, not a feature guard** — the sixth, and the one that tries to
//! FALSIFY a formula rather than to describe a residue.
//!
//! # The formula under test
//!
//! [`super::trace`] records that the emission count tracks the toggled region's child
//! count `N`, and that the sequence's second run SATURATES: `N` at ten children, 22 at
//! thirty. A reading of GTK 4.6.9 proposes why. `incremental_validate_callback`
//! (`gtktextview.c:4817`) calls `gtk_text_layout_validate(layout, 2000)` — a hard-coded
//! PIXEL budget — and `gtk_text_layout_validate` (`gtktextlayout.c:1033-1051`) emits
//! `::changed` once per validated CHUNK, decrementing that budget by the chunk's height.
//! Its guard is tested BEFORE the decrement, so a pass spans `ceil(2000 / chunk)` chunks;
//! [`super::budget`] is the arm that measures that directly. Each anchored child
//! invalidates one character, so `N` children spread through the body are `N` isolated
//! one-line chunks: the measured leading run of exactly `N`. The run after it is the body
//! itself, and at a 90 px chunk `ceil(2000 / 90) = 23` bounds it — of which 22 are the
//! uniform run and the 23rd is the shorter chunk that closes the pass.
//!
//! ```text
//! emissions = N + min(N, 22) + tail
//! ```
//!
//! It reproduces the counts already on the record (`0 -> 2`, `10 -> 23`, `30 -> 55`) but
//! was INFERRED from them, so it owes a prediction it can fail: a KINK at `N = 22/23`,
//! where each added child stops adding TWO emissions (32 px of drift at the configured
//! margin) and starts adding ONE. The doses straddle it; the three it was fitted to
//! cannot see it.
//!
//! # ⚠ THE COUNT IS BIMODAL, AND THAT IS WHY EACH DOSE IS RUN SEVERAL TIMES
//!
//! [`super::margin`] records the thirty-child cell coming back with **55 or 63**
//! emissions run to run, nothing changed between them. A kink is a change in STEP SIZE,
//! so one mode flip at one dose forges or erases an apparent kink on its own. Every dose
//! is therefore run [`REPEATS`] times, every mode reported with its frequency, and the
//! marginal step computed WITHIN a mode ([`steps`] pairs each dose's lowest count with
//! the next dose's lowest) — never across a mixture and never from an average, an
//! average of a bimodal distribution being a number that never occurred.
//!
//! # What it measured — THE KINK IS THERE, exactly where the formula put it
//!
//! GTK 4.6.9, X11/Xvfb, 700x600 pane, deferred parenting, expanding, configured 16 px
//! top-margin, this test run ALONE (see the last section). Four replicates of the grid,
//! so twenty runs per dose:
//!
//! | `N` | emissions (freq) | drift | up-leg | second mode (freq) |
//! |---|---|---|---|---|
//! | 5 | **13** (20/20) | 208 px | 5? | — |
//! | 20 | **43** (20/20) | 688 px | 20? | — |
//! | 22 | **47** (20/20) | 752 px | 22 | — |
//! | 23 | **48** (19/20) | 768 px | 22 | 49 (1/20) |
//! | 25 | **50** (18/20) | 800 px | 22 | 53 (2/20) |
//! | 30 | **55** (18/20) | 880 px | 22 | 63 (2/20) |
//! | 40 | **65** (16/20) | 1 040 px | 22 | 83 (4/20) |
//!
//! A `?` marks a run no terminal chunk closed, so the encoding cannot say how many
//! passes it holds — see the section after next. The two that carry one are the doses
//! BELOW the ceiling, where the up-leg stops because the body ran out rather than
//! because the budget did.
//!
//! The marginal step, within the lowest mode: **+2 emissions (+32 px) per child from 5
//! to 22, then +1 (+16 px) from 22 to 23 and at every dose above**, the break falling
//! between 22 and 23 and nowhere else. `emissions = N + min(N, 22) + 3` reproduces every
//! lowest mode to the emission, `drift = emissions x top_margin` ([`super::margin`]'s
//! identity) holds to 0.0 px in all 140 runs, and the up-leg column is read off the
//! sequence rather than inferred from the total.
//!
//! # ⚠ THE UP-LEG COLUMN IS A RUN LENGTH, AND ESTABLISHES A CEILING ONLY SOMETIMES
//!
//! It is NOT "the ceiling being hit directly", and this file said so for as long as it
//! existed. [`super::dose::UpLeg`] carries the account: consecutive validation passes
//! charging the same delta merge into ONE run of the encoding, so a run of `2P` and a run
//! of `P` are the same trace of a `P`-chunk pass and nothing in the encoding separates
//! them. A boundary is established only where a SHORTER terminal chunk closes the pass,
//! and MEASURED that is the doses at or above the ceiling and not the ones below it —
//! which is why this grid's numbers stood up, not a property of the reading itself. The
//! column prints a trailing `?` where no boundary was established; the saturation below
//! is taken only from the doses that established one, and where a dose did not, the
//! assertion weakens from an equality to a divisibility rather than inventing a number.
//!
//! **The SECOND mode is the same formula with the budget not biting**: every one
//! measured is exactly `2N + 3` — 49, 53, 63, 83 — `min(N, 22)` replaced by `N`. The
//! up-leg stops at 22 and then RESUMES for a further `N - 23`, a second validate pass
//! finishing what the first pass's budget cut off. One mechanism, one pass or two, which
//! is also why nothing below `N = 23` is bimodal. [`super::budget`] takes it further by
//! moving the chunk height.
//!
//! # ⚠ THE WHOLE GRID MOVES WITH THE AMBIENT FONT METRICS
//!
//! Run inside the full lib suite rather than alone, the same doses come back
//! DETERMINISTICALLY at 12 / 43 / 47 / 49 / 53 / 60 / 70 — a saturation of **27**, not
//! 22, which is `ceil(2000 / 74) - 1` against this table's `ceil(2000 / 90) - 1`, the
//! `- 1` being the shorter terminal chunk in each case. The formula
//! holds throughout, with a different chunk height in it; the numbers above do not. That
//! is why every assertion below is written against the saturation the RUN measured, and
//! why the recorded 55/63 is printed for comparison and never asserted — a literal here
//! would make this test order-dependent and would have been read as a refutation.
//!
//! The ceiling moves and so does where it falls in the grid: under the suite the doses
//! that establish a boundary are 30 and 40 alone, and 22 / 23 / 25 report `22?` / `23?` /
//! `25?` because 27 is above them. The grid still grades every dose — an equality at the
//! two that establish the ceiling, a divisibility at the five that do not.

use super::dose::{drift_at, modes, run, up_leg_at, Run};
use super::margin::configured_top_margin;
use super::recorder;

/// The doses. Five straddling the predicted kink at 22/23, plus **30 — the positive
/// control**, which must reproduce [`super::trace`]'s and [`super::margin`]'s measured
/// counts or nothing else here means anything.
const DOSES: [usize; 7] = [5, 20, 22, 23, 25, 30, 40];

/// The dose whose reading is compared against the record: it is the one
/// [`super::trace`] and [`super::margin`] already measured.
const CONTROL_DOSE: usize = 30;

/// The counts [`CONTROL_DOSE`] produced when this grid was run ALONE — reported beside
/// every run so a reader can see whether the ambient state has moved, and deliberately
/// NOT asserted. See this module's docs: the whole grid shifts under the full lib suite,
/// so a literal here would make the test order-dependent while proving nothing the
/// shapes below do not.
const RECORDED_CONTROL_MODES: [usize; 2] = [55, 63];

/// How many times each dose is run. Enough that a mode appearing in a minority of runs
/// is still seen: [`super::margin`] found the second mode in two runs of five.
const REPEATS: usize = 5;

/// Slack for the positive control, in pixels — about one text row, as elsewhere here.
const SLACK_PX: f64 = 12.0;

/// Tolerance for the `drift == emissions x top_margin` identity, in pixels. As tight as
/// [`super::margin`]'s, and for the same reason: it is an equality between whole-pixel
/// quantities, so the only error to absorb is one multiplication's.
const IDENTITY_EPSILON_PX: f64 = 1e-9;

/// Every run, in the order taken — the raw table, never aggregated away, because a mode
/// that appeared once is the reading a summary would delete.
fn per_run_report(runs: &[Run], top_margin: f64) -> String {
    let mut out = format!(
        "\n=== dose table: EXPANDING a disclosure ABOVE the reading position ===\n\
         \x20 {REPEATS} runs per dose, deferred parenting, top-margin {top_margin:.0}px\n\
         \x20 {:>4} {:>4} {:>10} {:>10} {:>10} {:>10}\n",
        "N", "run", "emissions", "drift", "predicted", "residual",
    );
    for (i, r) in runs.iter().enumerate() {
        out.push_str(&format!(
            "  {:>4} {:>4} {:>10} {:>8.1}px {:>8.1}px {:>8.1}px\n",
            r.dose,
            i % REPEATS + 1,
            r.count(),
            r.drift(),
            r.predicted_drift(top_margin),
            r.drift() - r.predicted_drift(top_margin),
        ));
    }
    out.push_str("\n  the logged deltas, as runs of consecutive identical values:\n");
    for r in runs {
        out.push_str(&format!(
            "    N={:<3} {:>3} emissions  {}\n",
            r.dose,
            r.count(),
            recorder::runs_summary(r.emissions()),
        ));
    }
    out
}

/// Every mode each dose produced, with its frequency — the reading a single sample per
/// cell would have replaced with one arbitrary member of it.
fn mode_report(runs: &[Run], top_margin: f64) -> String {
    let mut out = format!(
        "\n=== modes seen, per dose (every mode, with its frequency) ===\n\
         \x20 a `?` on the up-leg means no terminal chunk closed it, so the encoding \
         cannot say\n\x20 how many passes that run holds — see `dose::UpLeg`\n\
         \x20 {:>4} {:>10} {:>7} {:>10} {:>8}\n",
        "N", "emissions", "freq", "drift", "up-leg",
    );
    for dose in DOSES {
        for (count, freq) in modes(runs, dose) {
            out.push_str(&format!(
                "  {:>4} {:>10} {:>5}/{} {:>8.1}px {:>8}{}\n",
                dose,
                count,
                freq,
                REPEATS,
                drift_at(runs, dose, count),
                up_leg_at(runs, dose, count, top_margin).column(),
                if freq == REPEATS { "" } else { "   <-- SPLIT" },
            ));
        }
    }
    out.push_str(&format!(
        "  the control dose {CONTROL_DOSE} was recorded ALONE at \
         {RECORDED_CONTROL_MODES:?}; here {:?}\n",
        modes(runs, CONTROL_DOSE)
            .iter()
            .map(|(count, _)| *count)
            .collect::<Vec<_>>(),
    ));
    out
}

/// The marginal step between adjacent doses, computed **within a mode**: each dose's
/// LOWEST observed count paired with the next dose's lowest.
///
/// Never across a mixture and never from an average. A single mode flip is worth several
/// times the step being measured, so a step taken across two doses' mixtures can show a
/// change in step size that did not happen, or hide one that did.
fn steps(runs: &[Run], top_margin: f64) -> String {
    let mut out = format!(
        "\n=== marginal step, WITHIN the lowest mode of each dose ===\n\
         \x20 the discriminator: 2 emissions ({:.0}px) per child below the predicted \
         kink at N=22/23,\n\x20 1 emission ({:.0}px) at and above it\n\
         \x20 {:>10} {:>14} {:>10} {:>12} {:>14}\n",
        top_margin * 2.0,
        top_margin,
        "N -> N'",
        "emissions",
        "d(N)",
        "d(emissions)",
        "per child",
    );
    let lowest: Vec<(usize, usize)> = DOSES
        .iter()
        .map(|&dose| (dose, modes(runs, dose)[0].0))
        .collect();
    for pair in lowest.windows(2) {
        let [(from, from_count), (to, to_count)] = pair else {
            continue;
        };
        let d_dose = to - from;
        let d_emissions = *to_count as f64 - *from_count as f64;
        out.push_str(&format!(
            "  {:>10} {:>14} {:>10} {:>12.0} {:>9.2}/child ({:>+6.2}px)\n",
            format!("{from} -> {to}"),
            format!("{from_count} -> {to_count}"),
            d_dose,
            d_emissions,
            d_emissions / d_dose as f64,
            d_emissions / d_dose as f64 * top_margin,
        ));
    }
    out
}

/// **The measurement.** Every dose in one test, for the same reason every other grid in
/// this directory is one: the positive control and the doses it qualifies are only
/// meaningful against each other, and a control in a separate test can be filtered out.
///
/// What is ASSERTED is the rig — the positive control at [`CONTROL_DOSE`], the dose
/// arriving, the reader's anchor surviving, and the `drift == emissions x top_margin`
/// identity [`super::margin`] established. The kink itself is REPORTED and not asserted:
/// it is a property of this GTK's validation budget against this host's font metrics,
/// and pinning either would make this test a portability hazard rather than a record.
#[gtktest::test]
fn the_emission_count_against_the_validation_budget() {
    if super::skip_if_gtk_compensates_top_margin("drift: validation-budget kink") {
        return;
    }
    let mut runs: Vec<Run> = Vec::new();
    for dose in DOSES {
        for _ in 0..REPEATS {
            runs.push(run(dose));
        }
    }
    let top_margin = f64::from(configured_top_margin());

    let out = format!(
        "{}{}{}",
        per_run_report(&runs, top_margin),
        mode_report(&runs, top_margin),
        steps(&runs, top_margin),
    );
    println!("{out}");

    // ── The positive control, first, and it is the whole licence for the rest. ──
    //
    // SATURATION, not a literal count. The record's 55/63 at thirty children is reported
    // above and deliberately not asserted: the whole grid shifts with the ambient font
    // metrics (see this module's docs — under the full lib suite the same doses come
    // back on a different chunk height entirely), so pinning it would make this test
    // order-dependent while proving nothing the shapes below do not. What must be true
    // for a KINK to be observable at all is that the budget bit somewhere inside the
    // dose range: some dose's up-leg ran fewer chunks than that dose has children.
    //
    // Taken ONLY from the doses whose trace established a pass boundary — a run with no
    // terminal chunk may be several merged passes, and a maximum over such a run would
    // report a multiple of the ceiling as the ceiling. Where none establishes one, this
    // says so and stops, rather than reading a number the instrument cannot supply.
    let largest_dose = DOSES[DOSES.len() - 1];
    let saturation = DOSES
        .iter()
        .filter_map(|&dose| {
            up_leg_at(&runs, dose, modes(&runs, dose)[0].0, top_margin).established_prefix()
        })
        .max();
    let Some(saturation) = saturation else {
        panic!(
            "INSTRUMENT ERROR: not one dose in this grid closed its up-leg with a shorter \
             terminal chunk, so no pass boundary was established anywhere and the \
             saturation cannot be read off the encoding at all (a run with no terminal \
             chunk may be several merged passes). Nothing may be concluded about the \
             budget from this run.{out}"
        );
    };
    assert!(
        saturation < largest_dose,
        "THE RIG IS NOT EXERCISING THE PHENOMENON. The longest established pass ran \
         {saturation} chunks against a largest dose of {largest_dose}, so the validation \
         budget never bit anywhere in this grid and there is no kink for it to show. \
         Raise the doses until it does rather than reading the flat steps below as a \
         refutation.{out}"
    );

    // ── The dose-response itself, without which a count is not about children. ──
    let smallest = modes(&runs, DOSES[0])[0].0;
    let largest = modes(&runs, largest_dose)[0].0;
    assert!(
        largest > smallest + DOSES[0],
        "THE RIG IS NOT EXERCISING THE PHENOMENON. The emission count must grow with \
         the child count (MEASURED 13 at five and 65 at forty), and here {} children \
         gave {smallest} against {largest_dose} children's {largest}.{out}",
        DOSES[0],
    );
    let control_drift = drift_at(&runs, CONTROL_DOSE, modes(&runs, CONTROL_DOSE)[0].0);
    assert!(
        control_drift > SLACK_PX,
        "THE RIG IS NOT EXERCISING THE PHENOMENON. The reader must drift at all for a \
         step in that drift to be measurable, and at {CONTROL_DOSE} children they moved \
         {control_drift:.1}px.{out}"
    );

    // ── The identity, at every run of the grid. ────────────────────────────────
    //
    // `super::margin`'s finding, re-checked here rather than assumed: it is what lets a
    // step in the EMISSION COUNT be read as a step in the reader's drift, which is the
    // whole claim the doses above are testing. A residual would mean the two are no
    // longer the same measurement.
    for r in &runs {
        let residual = r.drift() - r.predicted_drift(top_margin);
        assert!(
            residual.abs() <= IDENTITY_EPSILON_PX,
            "drift = emissions x top_margin does not hold at {} children: {} emissions \
             at {top_margin:.0}px predict {:.1}px and the reader drifted {:.1}px, a \
             residual of {residual:.1}px. `margin` records this identity as EXACT, so a \
             residual relocates the mechanism and the step table above is measuring two \
             quantities rather than one.{out}",
            r.dose,
            r.count(),
            r.predicted_drift(top_margin),
            r.drift(),
        );
    }

    // ── The finding, half one: the up-leg is `min(N, saturation)`. ────────────
    //
    // The formula's load-bearing term, observed rather than fitted — and the reason the
    // kink is where it is. Stated against the saturation this run MEASURED rather than
    // against 22, so it survives a host whose lines are a different height (which is
    // exactly what the full lib suite turns out to be).
    //
    // Graded to what each dose's trace can establish, and no further: an EQUALITY where a
    // terminal chunk closed the up-leg, and a DIVISIBILITY where none did, because that
    // run may hold several passes the encoding cannot separate. Asserting the equality
    // everywhere would fail on a merge that the model itself predicts.
    for &dose in &DOSES {
        let leg = up_leg_at(&runs, dose, modes(&runs, dose)[0].0, top_margin);
        let expected = dose.min(saturation);
        match leg.established_prefix() {
            Some(prefix) => assert_eq!(
                prefix, expected,
                "the up-leg at {dose} children closed after {prefix} chunks where the \
                 budget model gives min({dose}, {saturation}) — so the run does NOT track \
                 the dose up to a fixed ceiling, which is the whole term the kink is a \
                 consequence of.{out}"
            ),
            None => assert!(
                expected > 0 && leg.run.is_multiple_of(expected),
                "the up-leg at {dose} children ran {} chunks with no terminal chunk to \
                 close it, and {} is not a whole number of the min({dose}, {saturation}) \
                 the budget model gives — so it is neither one pass of the predicted \
                 length nor several of them, which is the term the kink is a consequence \
                 of failing.{out}",
                leg.run,
                leg.run,
            ),
        }
    }

    // ── Half two: the marginal step is TWO below the saturation and ONE above. ─
    //
    // The discriminator. Below the ceiling each child adds a chunk to both runs; above
    // it, only to the leading one. Taken WITHIN the lowest mode of each dose (never
    // across a mixture — see this module's docs) and rounded to a whole emission per
    // child, which is the granularity of the claim: the tail carries a small constant
    // that can wobble by one write, and over a dose interval that is what a fraction of
    // a step means. The exact figures are in the printed table.
    let lowest = |dose: usize| modes(&runs, dose)[0].0 as f64;
    let mut below = 0usize;
    let mut above = 0usize;
    for pair in DOSES.windows(2) {
        let [from, to] = pair else { continue };
        let step = (lowest(*to) - lowest(*from)) / (to - from) as f64;
        let expected = if *to <= saturation {
            below += 1;
            2.0
        } else if *from >= saturation {
            above += 1;
            1.0
        } else {
            // The pair that STRADDLES the ceiling: part of the interval steps by two
            // and part by one, so it predicts neither and is deliberately not graded.
            continue;
        };
        assert_eq!(
            step.round(),
            expected,
            "between {from} and {to} children each child added {step:.2} emissions, where \
             a budget saturating at {saturation} predicts {expected:.0}. This is the \
             discriminator the grid exists for, so a mismatch is a REFUTATION of the \
             formula rather than a tolerance to widen — re-read the tables above and \
             update this module's docs.{out}"
        );
    }
    assert!(
        below > 0 && above > 0,
        "the grid graded {below} dose interval(s) below the saturation at {saturation} \
         and {above} above it, so it cannot see a change in step size at all and the \
         assertions above are vacuous on one side.{out}"
    );
}
