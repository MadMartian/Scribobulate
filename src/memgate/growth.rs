//! Does this series step once, or climb? — the per-render growth predicate.
//!
//! Display-free: the GTK driver feeds it a slice of footprint readings; this
//! module never opens a process or a file. That is what lets the assertion
//! itself be unit-tested with planted series — a climbing series must fail,
//! a stepped one must pass, wherever the step lands — so a later change to the
//! arithmetic cannot silently invert the gate (GEP-1 / GEP-1).
//!
//! **Why not a half-mean.** This predicate replaces `mean(second half) −
//! mean(first half)`, which cannot tell the two apart at all: one allocation
//! retained for the rest of the run raises the second-half mean exactly as a
//! per-render leak does, and by an amount that depends on WHERE in the run it
//! landed. Measured on the Linux CI runner, one ~12.6 MB step (the kernel
//! collapsing heap into huge pages, not an allocation) reported an
//! 8,650,512-byte delta arriving at sample 20 of 69 and 2,165,459 bytes arriving
//! at sample 5 — pass or fail decided by the allocation's timing rather than the
//! program's growth. The leak this class exists for (ScrAP-351) is ~12 MB *per
//! render*, the same magnitude as that single step, so no tolerance separates
//! them.
//!
//! **What separates them is how much of the growth ONE allocation explains.**
//! Two clauses, neither of which can see where in the run anything happened:
//!
//! * **Residual growth** — total growth minus the largest single adjacent rise.
//!   A quantity allocated once is entirely accounted for by that subtraction,
//!   however large it is and wherever it lands; a quantity added on every render
//!   leaves `(n − 1)` of itself behind. On the measured traces the two are two
//!   orders of magnitude apart, not a knife edge.
//! * **Total growth** — an absolute ceiling, because residual alone would pass a
//!   single arbitrarily large allocation (TDD 6.12).
//!
//! Sensitivity follows the window rather than a fixed per-render figure: a climb
//! of `x` per sample over `n` samples leaves `(n − 1)·x` of residual, so a longer
//! window sees a smaller leak. That is the property ScrAP-351 records — growth
//! visible only across many repetitions — stated as arithmetic.

/// The two thresholds a series is judged against. Per-platform — see
/// [`super::footprint::GROWTH_BOUNDS`], which is their one owner.
#[derive(Clone, Copy)]
pub(crate) struct Bounds {
    /// Ceiling on growth that ONE allocation does not explain. Above a clean
    /// trace's churn, far below what any per-render climb leaves behind.
    pub(crate) residual_bytes: u64,
    /// Ceiling on total growth, above any plausible one-time allocation. This is
    /// the clause that stops "only one allocation" excusing an unbounded one.
    pub(crate) total_bytes: u64,
}

/// Discard the first `warmup` readings. Allocator and loader warm-up dominate
/// those; a leak is what keeps climbing after they have saturated.
pub(crate) fn after_warmup(samples: &[u64], warmup: usize) -> &[u64] {
    if warmup >= samples.len() {
        &[]
    } else {
        &samples[warmup..]
    }
}

/// The largest single adjacent rise in the series. A fall is not a negative
/// rise: footprint drops when the allocator returns pages, and subtracting that
/// would credit the run for memory it never held.
pub(crate) fn largest_rise(samples: &[u64]) -> u64 {
    samples
        .windows(2)
        .map(|pair| pair[1].saturating_sub(pair[0]))
        .max()
        .unwrap_or(0)
}

/// How much more memory is held at the END of the window than at its start.
///
/// Deliberately last − first rather than max − min: the question is what the run
/// RETAINED, and a transient peak the allocator gave back is not retention. A
/// climb makes the two agree anyway.
pub(crate) fn total_growth(samples: &[u64]) -> i64 {
    match (samples.first(), samples.last()) {
        (Some(&first), Some(&last)) => last as i64 - first as i64,
        _ => 0,
    }
}

/// Growth that the single largest allocation does not account for.
///
/// This is the whole shape test. One retained allocation, flat either side,
/// leaves zero here whatever its size and wherever it sits; `x` added on every
/// one of `n` samples leaves `(n − 1)·x`.
pub(crate) fn residual_growth(samples: &[u64]) -> i64 {
    total_growth(samples) - largest_rise(samples) as i64
}

/// `Ok(())` when the series after `warmup` grew no more than
/// `bounds.residual_bytes` beyond what its largest single rise explains, and
/// retained no more than `bounds.total_bytes` in all.
pub(crate) fn assert_no_growth(
    samples: &[u64],
    warmup: usize,
    bounds: Bounds,
) -> Result<(), String> {
    let rest = after_warmup(samples, warmup);
    if rest.len() < 2 {
        return Err(format!(
            "need at least {} samples after warmup={warmup} (got {})",
            warmup + 2,
            samples.len()
        ));
    }
    let total = total_growth(rest);
    let rise = largest_rise(rest);
    let residual = residual_growth(rest);
    if residual > bounds.residual_bytes as i64 {
        return Err(format!(
            "footprint grew {total} bytes across {} samples, of which one allocation of \
             {rise} bytes explains only part: {residual} bytes of growth remain, over the \
             {} byte residual bound — that is a climb, not a step; \
             samples after warmup: {rest:?}",
            rest.len(),
            bounds.residual_bytes
        ));
    }
    if total > bounds.total_bytes as i64 {
        return Err(format!(
            "footprint grew {total} bytes across {} samples, over the {} byte ceiling — \
             too large to be one-time warm-up however few allocations it took \
             (largest single rise {rise} bytes); samples after warmup: {rest:?}",
            rest.len(),
            bounds.total_bytes
        ));
    }
    Ok(())
}

/// One line saying what the series did, for the run log.
///
/// A gate that reports nothing on success leaves nothing to compare against when
/// it later fails, which is exactly what made the CI failure this predicate
/// replaces expensive to read: the only traces anyone had came out of the
/// failure message. It also keeps the bounds derivable from a passing run on any
/// host, rather than only from a red one.
pub(crate) fn describe(samples: &[u64], warmup: usize, bounds: Bounds) -> String {
    let rest = after_warmup(samples, warmup);
    format!(
        "{} samples after warmup={warmup}: total growth {} bytes, largest rise {} bytes, \
         residual {} bytes (bounds: residual {}, total {})",
        rest.len(),
        total_growth(rest),
        largest_rise(rest),
        residual_growth(rest),
        bounds.residual_bytes,
        bounds.total_bytes
    )
}

#[cfg(test)]
mod tests {
    use super::{
        after_warmup, assert_no_growth, describe, largest_rise, residual_growth, total_growth,
        Bounds,
    };

    /// Round numbers, not any platform's: these exercise the arithmetic, and a
    /// test that borrowed the shipped constants would stop failing when one of
    /// them was mis-edited.
    const BOUNDS: Bounds = Bounds {
        residual_bytes: 1_000,
        total_bytes: 20_000,
    };

    /// ScrAP-351's leak in miniature: a quantity added on every single sample.
    fn climbing(len: usize, per_sample: u64) -> Vec<u64> {
        (0..len as u64).map(|i| 100_000 + i * per_sample).collect()
    }

    /// Flat, one rise of `size` arriving at `at`, flat again to the end.
    fn stepped(len: usize, at: usize, size: u64) -> Vec<u64> {
        (0..len)
            .map(|i| if i < at { 100_000 } else { 100_000 + size })
            .collect()
    }

    #[test]
    fn after_warmup_drops_the_prefix() {
        assert_eq!(after_warmup(&[1, 2, 3, 4, 5], 2), &[3, 4, 5]);
    }

    #[test]
    fn after_warmup_empty_when_warmup_covers_all() {
        assert!(after_warmup(&[1, 2], 2).is_empty());
        assert!(after_warmup(&[1], 5).is_empty());
    }

    #[test]
    fn largest_rise_takes_the_biggest_climb_and_ignores_the_falls() {
        assert_eq!(largest_rise(&[10, 20, 5, 700, 700]), 695);
        assert_eq!(largest_rise(&[700, 10]), 0);
        assert_eq!(largest_rise(&[]), 0);
    }

    #[test]
    fn total_growth_is_last_minus_first() {
        assert_eq!(total_growth(&[10, 500, 40]), 30);
        assert_eq!(total_growth(&[500, 10]), -490);
        assert_eq!(total_growth(&[]), 0);
    }

    #[test]
    fn residual_of_one_step_is_nothing_and_of_a_climb_is_most_of_it() {
        assert_eq!(residual_growth(&stepped(8, 3, 12_600)), 0);
        // Ten samples climbing by 100: total 900, largest rise 100.
        assert_eq!(residual_growth(&climbing(10, 100)), 800);
    }

    /// TDD 6.11, first half: a one-time step passes WHEREVER it lands. This is
    /// the whole defect the half-mean had — its verdict moved with the step's
    /// position — so the position is swept rather than sampled.
    #[test]
    fn a_single_step_passes_at_every_position() {
        let len = 12;
        for at in 0..len {
            let series = stepped(len, at, 12_600);
            assert_no_growth(&series, 0, BOUNDS).unwrap_or_else(|err| {
                panic!("a one-time step at sample {at} of {len} must pass: {err}")
            });
        }
    }

    /// And that holds for a step far larger than the residual bound, which is
    /// the case the CI runner produced (~12.6 MB of huge-page collapse) against a
    /// 2 MB tolerance.
    #[test]
    fn a_single_step_larger_than_the_residual_bound_still_passes() {
        for at in 0..10 {
            let series = stepped(10, at, BOUNDS.residual_bytes * 15);
            assert_no_growth(&series, 0, BOUNDS)
                .unwrap_or_else(|err| panic!("step at {at}: {err}"));
        }
    }

    /// TDD 6.11, second half: a per-render climb fails, and says what it
    /// measured. The reason matters — a mutation that broke the predicate could
    /// still "fail" on the length precondition and look live (GEP-11).
    #[test]
    fn a_per_render_climb_fails_and_names_its_growth() {
        let series = climbing(13, 12_000);
        let err = assert_no_growth(&series, 3, BOUNDS).unwrap_err();
        assert!(
            err.contains("footprint grew 108000 bytes")
                && err.contains("96000 bytes of growth remain")
                && err.contains("a climb, not a step"),
            "failed for the wrong reason or named no growth: {err}"
        );
        assert!(!err.contains("need at least"), "precondition fired: {err}");
    }

    /// A climb small enough that no single rise is remarkable is exactly what
    /// the gate exists for, and residual keeps its sensitivity there: ten
    /// samples rising by a ninth of the bound each still exceed it together.
    #[test]
    fn a_climb_of_small_rises_fails_on_their_sum() {
        let per_sample = BOUNDS.residual_bytes / 4;
        let series = climbing(13, per_sample);
        assert!(
            assert_no_growth(&series, 3, BOUNDS).is_err(),
            "nine rises of {per_sample} must not pass a residual bound of {}",
            BOUNDS.residual_bytes
        );
    }

    /// TDD 6.12: one allocation, but too large to be warm-up. Residual cannot
    /// see this by construction — the ceiling is what catches it.
    #[test]
    fn a_single_step_over_the_ceiling_fails() {
        let series = stepped(12, 4, BOUNDS.total_bytes + 1);
        let err = assert_no_growth(&series, 0, BOUNDS).unwrap_err();
        assert!(
            err.contains("over the 20000 byte ceiling"),
            "failed for the wrong reason: {err}"
        );
    }

    /// Two large allocations are one more than the residual clause forgives, so
    /// a leak arriving in few big chunks is still caught.
    #[test]
    fn two_large_allocations_fail_on_residual() {
        let mut series = vec![100_000; 12];
        for s in series.iter_mut().skip(4) {
            *s += 9_000;
        }
        for s in series.iter_mut().skip(8) {
            *s += 9_000;
        }
        assert!(assert_no_growth(&series, 0, BOUNDS).is_err());
    }

    /// A plateau of identical readings — run 2 of the CI traces ended with 49
    /// of them — is the healthiest series there is.
    #[test]
    fn a_plateau_passes() {
        assert_no_growth(&[100, 180, 200, 200, 201, 200, 200, 201], 2, BOUNDS).unwrap();
    }

    /// Footprint that falls, because the allocator returned pages, is not growth
    /// in either direction.
    #[test]
    fn a_falling_series_passes() {
        assert_no_growth(&[900, 800, 700, 400, 100], 0, BOUNDS).unwrap();
    }

    #[test]
    fn a_short_series_is_named_as_a_precondition() {
        let err = assert_no_growth(&[1, 2], 2, BOUNDS).unwrap_err();
        assert!(err.contains("need at least"), "{err}");
    }

    #[test]
    fn describe_names_the_window_the_growth_and_the_bounds() {
        let line = describe(&stepped(6, 3, 12_600), 2, BOUNDS);
        assert!(
            line.contains("4 samples after warmup=2")
                && line.contains("total growth 12600 bytes")
                && line.contains("largest rise 12600 bytes")
                && line.contains("residual 0 bytes")
                && line.contains("bounds: residual 1000, total 20000"),
            "{line}"
        );
    }
}
