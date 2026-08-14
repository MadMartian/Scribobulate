//! Test-only wall-clock sampling, shared by every guard that asserts a growth RATIO.
//!
//! # Why this is a module and not a helper in one test file
//!
//! The same reason `testsymlink` is one. Two guards in this tree assert an algorithmic
//! exponent by timing two input sizes — `annotate::scan`'s (QA R3 D-2, the scan-per-opener
//! quadratic) and `renderer::normalize`'s (QA R3 D-3, the per-tab backwards line walk) —
//! and the noise remedy was written into ONE of them. `annotate::scan` grew a documented
//! best-of-5 sampler; `renderer::normalize` kept a single `Instant::now()` draw and has no
//! mitigation at all. Both then failed on hosted CI runners within one run of each other,
//! at 8.6x and 8.7x against a threshold of 8.0, with the code correct. A remedy that lives
//! inside one consumer is a remedy the next consumer will not find.
//!
//! # Why the MINIMUM, and why more samples rather than a looser threshold
//!
//! Timing noise on a shared machine is strictly ADDITIVE: preemption, cache eviction and
//! frequency scaling can only make a run slower than the work actually costs, never faster.
//! The floor of the observed distribution is therefore the estimate of the noise-free cost,
//! while any single sample is one arbitrary draw from a right-skewed one — and a mean folds
//! the outliers back in, which is exactly what a ratio between two samples must not do. The
//! minimum also discards first-call warm-up for free.
//!
//! That is what makes SAMPLE COUNT the right knob for a noisy machine and the threshold the
//! wrong one. A hosted runner does not invalidate the estimator; it just needs more draws
//! to find the same floor. Widening the threshold instead would spend the discriminating
//! power the guard exists for — measured, the numbers leave no room for it:
//!
//! ```text
//! linear 4.0 ─ quiet box 3.10–5.05 ─ threshold 8.0 ─ hosted CI 8.6/8.7 ─ quadratic 16.0
//! ```
//!
//! (80 samples on a real Windows host: min 3.10, avg 4.03 against a textbook-linear 4.0,
//! max 5.05. A threshold set to clear CI's 8.6 would sit inside the quadratic band it is
//! supposed to catch.)
//!
//! # The knob
//!
//! `SCRIBTEST_TIMING_SAMPLES` overrides [`DEFAULT_SAMPLES`]. Test-scoped by name on
//! purpose: `SCRIB_*` in this tree means a real build variable (`SCRIB_GTK_PREFIX`,
//! `SCRIB_GIT_COMMIT`), and a knob that can only ever loosen a test must not read like one
//! of those. It is set on the CI execution jobs and nowhere else, so a developer running
//! `cargo test` gets the tight default.
//!
//! An override ANNOUNCES ITSELF on stderr, following the house rule that an operator
//! override says so in the output (`pipeline.ps1 -SkipIntegration` does the same). A green
//! run under a raised sample count must not be mistakable for a green run under the
//! default — the assertion is identical either way, but how hard the machine worked to
//! satisfy it is not, and that belongs in the log rather than in someone's memory.

use std::time::Duration;

/// Samples taken when nothing overrides it. What a developer's `cargo test` uses.
pub(crate) const DEFAULT_SAMPLES: usize = 5;

/// How many draws [`best_of`] should take.
///
/// Reads `SCRIBTEST_TIMING_SAMPLES`; falls back to [`DEFAULT_SAMPLES`] when unset, empty,
/// unparseable or zero. A malformed value is deliberately NOT a failure: this knob exists
/// to make a noisy machine's run more reliable, and a typo in CI config that turned every
/// timing guard into a hard error would be a worse outcome than quietly using the default.
/// It is announced either way, so a typo is visible rather than silent.
pub(crate) fn samples() -> usize {
    const VAR: &str = "SCRIBTEST_TIMING_SAMPLES";
    match std::env::var(VAR) {
        Ok(raw) => match raw.trim().parse::<usize>() {
            Ok(n) if n > 0 => {
                eprintln!("[{VAR}] taking {n} timing samples (default {DEFAULT_SAMPLES})");
                n
            }
            _ => {
                eprintln!(
                    "[{VAR}] ignoring unusable value {raw:?}; using the default \
                     {DEFAULT_SAMPLES}"
                );
                DEFAULT_SAMPLES
            }
        },
        Err(_) => DEFAULT_SAMPLES,
    }
}

/// Best (minimum) elapsed time of [`samples()`] runs of `f`.
///
/// `stop_early` is the caller's escape hatch for the FAILURE case: sampling must not make a
/// red run more expensive than the bug it is reporting. `annotate::scan`'s pre-fix cost was
/// ~96 s per call, so a plain best-of-N over five delimiter pairs turns one regression into
/// a multi-hour run — measured; the first attempt at that guard's fix had to be killed.
/// Returning `true` abandons the remaining draws. It is sound ONLY where the caller's
/// ceiling has wide headroom over the linear cost, so that a sample above it means a real
/// regression rather than a slow draw; do not reuse the shape for a tight bound, where it
/// would reintroduce the very flake this module exists to remove.
pub(crate) fn best_of(mut f: impl FnMut(), stop_early: impl Fn(Duration) -> bool) -> Duration {
    let mut best = Duration::MAX;
    for _ in 0..samples() {
        let t = std::time::Instant::now();
        f();
        best = best.min(t.elapsed());
        if stop_early(best) {
            break;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default is what an unset environment produces. Pinned because every guard's
    /// threshold was calibrated against it, so a change here silently re-aims all of them.
    #[test]
    fn an_unset_environment_uses_the_default() {
        // Not asserted via `samples()`: this suite may itself be running under an override
        // (that is the entire point of the knob), and a test that reads the ambient
        // environment would then assert about the CI config rather than about this code.
        assert_eq!(DEFAULT_SAMPLES, 5);
    }

    /// `best_of` must return the FLOOR, not the last or the mean draw — the whole estimator
    /// rests on it, and a `max`/`min` slip would leave every guard measuring noise while
    /// still passing on a quiet machine.
    #[test]
    fn best_of_returns_the_minimum_draw_not_the_last() {
        let mut n = 0u32;
        let d = best_of(
            || {
                n += 1;
                // First call sleeps, later ones do not: if this returned the last or the
                // mean draw the assertion below could not tell the difference.
                if n == 1 {
                    std::thread::sleep(Duration::from_millis(20));
                }
            },
            |_| false,
        );
        assert!(n >= 2, "sampling must take more than one draw");
        assert!(
            d < Duration::from_millis(20),
            "best_of returned {d:?}, which includes the deliberately slow first draw"
        );
    }

    /// The early exit must actually stop, or the failure-case cost this module documents is
    /// not bounded at all.
    #[test]
    fn stop_early_abandons_the_remaining_draws() {
        let mut n = 0u32;
        best_of(|| n += 1, |_| true);
        assert_eq!(n, 1, "stop_early must abandon after the first draw");
    }
}
