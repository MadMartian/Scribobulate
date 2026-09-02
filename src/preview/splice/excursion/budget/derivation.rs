//! **[`super`]'s arithmetic, replayed** over recorded encodings rather than
//! driven from a fresh window.
//!
//! The live merge is INTERMITTENT (see [`super::super::dose::UpLeg`]), so the case that broke
//! the previous version of that file cannot be relied on to appear in a run — and a case
//! that appears sometimes is not a regression guard. These bodies feed the derivation the
//! exact encodings that were measured, which makes the merge deterministic.
//!
//! Its own file rather than a `mod` block inside [`super`], which the addition
//! would have pushed past the 500-line soft limit (POLICY § Code style). The cut is by
//! cause: that file owns DRIVING the rig and reporting what it saw, and this one owns
//! proving the arithmetic it applies to the reading.

use super::super::dose::up_leg;
use super::super::recorder::Emission;
use super::{Arm, DOCUMENTED_BUDGET_PX};

const TOP_MARGIN_PX: f64 = 16.0;

/// Build an arm from a run-length encoding, exactly as the measurement does.
fn arm(filler: &'static str, runs: &[(usize, f64)]) -> Arm {
    let mut value = 0.0;
    let mut emissions = Vec::new();
    for &(count, delta) in runs {
        for _ in 0..count {
            value += delta;
            emissions.push(Emission { value, delta });
        }
    }
    let leg = up_leg(&emissions, TOP_MARGIN_PX);
    Arm {
        filler,
        leg,
        emissions,
    }
}

/// The half filler, both encodings it produced. A budget of 2 000 over a 72 px chunk
/// gives `ceil = 28`, so the merged run of 56 is TWO passes and the single run of 28
/// is one — and NEITHER establishes a boundary, so neither contributes a bracket.
///
/// This is the whole defect in one body: the previous derivation read 56 as a pass
/// length, and `56 * 72 = 4032` against the other arms' upper bounds is the empty
/// `[4032, 2032)` it then printed with a verdict attached.
#[gtktest::test]
fn a_merged_run_is_decomposed_and_brackets_nothing() {
    let merged = arm("half", &[(60, -7.0), (1, -88.0), (56, 56.0), (1, 344.0)]);
    assert_eq!(merged.leg.chunk, 72.0);
    assert_eq!(merged.pass_length(), 28, "ceil(2000/72), not floor's 27");
    assert_eq!(merged.predicted_prefix(), 28);
    assert_eq!(merged.passes(), Some(2));
    assert_eq!(merged.observed_prefix(), Some(28));
    assert_eq!(
        merged.bracket(),
        None,
        "no terminal chunk, so no boundary and no bracket — the step that produced \
         the empty interval"
    );

    let single = arm(
        "half",
        &[(60, -7.0), (1, -88.0), (28, 56.0), (1, 1514.0), (1, 830.0)],
    );
    assert_eq!(single.passes(), Some(1));
    assert_eq!(
        single.observed_prefix(),
        merged.observed_prefix(),
        "one pass and two must decompose to the same per-pass prefix, or the \
         instrument still reads a run length as a pass length"
    );
}

/// The two arms that DO establish a boundary, and the bracket they agree on. Derived
/// from the measured heights and the loop rule — `p*h < B <= p*h + s` — so the
/// documented constant is something this can be checked against rather than an input.
#[gtktest::test]
fn the_bracketing_arms_contain_the_documented_budget() {
    let standing = arm(
        "standing",
        &[(60, -7.0), (1, -88.0), (22, 74.0), (1, 56.0), (1, 1604.0)],
    );
    let double = arm(
        "double",
        &[(60, -7.0), (1, -88.0), (15, 111.0), (1, 93.0), (1, 1762.0)],
    );
    assert_eq!(standing.pass_length(), 23, "ceil(2000/90)");
    assert_eq!(
        standing.predicted_prefix(),
        22,
        "one less for the 72px close"
    );
    assert_eq!(standing.bracket(), Some((1980.0, 2052.0)));
    assert_eq!(double.pass_length(), 16, "ceil(2000/127)");
    assert_eq!(double.predicted_prefix(), 15);
    assert_eq!(double.bracket(), Some((1905.0, 2014.0)));

    let (lo, hi) = (1980.0_f64.max(1905.0), 2052.0_f64.min(2014.0));
    assert!(lo < hi, "the two arms' brackets intersect at ({lo}, {hi}]");
    assert!(
        lo < DOCUMENTED_BUDGET_PX && DOCUMENTED_BUDGET_PX <= hi,
        "the documented {DOCUMENTED_BUDGET_PX} must lie inside ({lo}, {hi}]"
    );
}

/// The `floor` framing this file used to carry, shown failing on the arm where the
/// two formulas differ. Kept as a body rather than a comment because the two agree at
/// the other two heights, which is exactly why the error survived being read.
#[gtktest::test]
fn the_floor_framing_does_not_divide_the_half_arms_run() {
    let half = arm("half", &[(60, -7.0), (1, -88.0), (28, 56.0), (1, 344.0)]);
    let floor = (DOCUMENTED_BUDGET_PX / half.leg.chunk).floor() as usize;
    assert_eq!(floor, 27);
    assert_ne!(
        floor,
        half.predicted_prefix(),
        "floor and ceil must differ at a 72px chunk, or this body proves nothing"
    );
    assert!(
        !half.leg.run.is_multiple_of(floor),
        "28 chunks is not a whole number of {floor}-chunk passes, which is the \
         falsification the divisibility check performs"
    );
}
