//! Process footprint sampler — one function, three `cfg` bodies.
//!
//! The returned number is **not RSS**. Linux reads `VmRSS`, macOS reads
//! `ri_phys_footprint`, Windows reads `PrivateUsage` less what the process heaps hold
//! committed but free. A shared name invites
//! a shared threshold; the bounds a series is judged against live next to this
//! module in [`GROWTH_BOUNDS`], and [`assert_bounded`] is the only thing that
//! reads them.
//!
//! **Windows reads private commit, not the working set**, because the working set
//! moves with no allocation at all. MEASURED on GTK 4.24.1: after the SVG gate's peak,
//! Windows trims the process and the PNG control's series faults those pages back in —
//! ~19k page faults, the working set +5–6 MB across the window while private commit rose
//! ~1.0–1.3 MB — so the control failed the residual bound in about one group run in five
//! on growth no code made. Private commit is allocation-driven, like macOS's
//! `ri_phys_footprint`.
//!
//! **And it subtracts the heaps' free committed bytes**, because the NT heap keeps freed
//! memory committed and never gives it back. MEASURED, 6.13's window gate, release build:
//! `PrivateUsage` stepped up 2–5 MB at a different cycle each run and never fell while the
//! heaps' live blocks grew ~21 KB a cycle, failing 3 runs in 10 (residual up to 8.7 MB)
//! on a build that retains nothing; with the subtraction, 10 runs in 10 left at most
//! 0.16 MB of residual. `platform::win32::process::current_allocated_private_bytes` says
//! how the free bytes are read, and `memgate::instrument` holds the reading to a known
//! leak and a known free.

/// Per-platform bounds for [`super::growth::assert_no_growth`], in bytes.
///
/// The one owner of both numbers, and [`assert_bounded`] the only reader.
///
/// **Both are derived from clean traces, and the run log carries the numbers
/// they were derived from.** `residual_bytes` sits above the growth a clean run
/// leaves unexplained by its single largest allocation, and far below what a
/// per-render climb leaves: ScrAP-351's leak is ~1.05 MB per render at the test
/// fixture's scale (~12 MB on a real document's image), which over a ten-sample
/// window — nine rises, less the largest — leaves ~8.4 MB of residual, about 2.7
/// times the bound rather than beside it, so no host's churn sits near the
/// decision. `total_bytes` sits above the largest one-time step
/// ever measured (~12.6 MB on the Linux CI runner, before `measuring()` stopped
/// the kernel's huge-page collapse from producing it) and far below what any
/// climb totals.
///
/// Measured clean traces, worst of the six gates in each run — the playback gate
/// on every host, being the only one with a live frame clock:
///
/// | host | total growth | largest rise | residual |
/// |------|--------------|--------------|----------|
/// | Linux development host, 4 runs | 1.04 MB | 0.70-0.77 MB | 0.27 MB |
/// | Linux CI runner, huge-page collapse armed | 12.98 MB | 12.08 MB | **0.86 MB** |
/// | Linux CI runner, collapse disabled | −0.38 MB | 0.38 MB | −0.77 MB |
/// | macOS CI runner | 0.31 MB | 0.31 MB | 0 |
/// | Windows CI runner (working set, before `PrivateUsage`) | 0.26 MB | 0.53 MB | −0.28 MB |
/// | macOS seat, per-stretch rule, 11 runs | 1.2–2.3 MB | 0.5–1.6 MB | **0.02–1.25 MB** |
/// | Windows seat, `PrivateUsage`, GTK 4.24.1, 10 runs | ≤1.93 MB | ≤1.15 MB | **≤1.11 MB** |
/// | Windows seat, `PrivateUsage`, GTK 4.22.4, 5 runs | ≤1.58 MB | ≤1.32 MB | ≤0.47 MB |
/// | Windows seat, `PrivateUsage` less free heap, 6.13 window gate alone, 10 runs | ≤0.24 MB | ≤0.08 MB | ≤0.16 MB |
///
/// The two Windows-seat `PrivateUsage` rows are worst-of-every-gate over every run; the
/// last row is the one gate whose failures the free-heap subtraction was made for. The
/// bounds did
/// not move for them. The gate still bites under `PrivateUsage`: retaining every decode
/// in 6.9's loop left 8.9 MB (animated WebP) and 69.5 MB (PNG) of residual.
///
/// **The per-stretch rule (see `growth::residual_growth`) raised the bound from 2 MiB to
/// 3 MiB** (operator decision, 2026-10-02). Judging every stretch rather than first to
/// last stops a fall from hiding a climb, and it also stops a fall from hiding slow
/// drift: the macOS playback gate's residual went from ~0.15 MB to as much as 1.25 MB
/// (59% of 2 MiB), mostly over stretches spanning nearly the whole window. At 3 MiB that
/// reading sits at ~40% while the fixture leak below still clears the bound ~2.7x.
///
/// **The armed CI row is the one that decided the bound**, and it is the trace
/// this predicate exists for: a ~12 MB one-time step, at a different sample each
/// run, which the half-mean this replaced reported as an 8.65 MB climb. It was
/// never an allocation — `khugepaged` filling heap pages the process already
/// owned (see `measuring()`) — but a step the program did not make is exactly
/// the shape the predicate must pass. Its 0.86 MB of residual is the worst
/// clean reading under the end-to-end rule, and it sits 3.6x under the bound while the
/// smallest leak the gate must catch — ScrAP-351's ~1.05 MB per render over a
/// ten-sample window, ~8.4 MB of residual — sits about 2.7x over it. Windows shows the bound must tolerate a NEGATIVE
/// residual: its footprint falls across the window, which is not growth.
/// A leak arriving in two chunks rather than one is the shape this cannot see;
/// the ceiling below is what bounds it.
pub(crate) const GROWTH_BOUNDS: super::growth::Bounds = super::growth::Bounds {
    residual_bytes: 3 * 1024 * 1024,
    total_bytes: 24 * 1024 * 1024,
};

/// ScrAP-351's per-render leak at the test fixture's scale, in bytes.
const FIXTURE_LEAK_PER_RENDER: u64 = 1_050_000;

/// The bounds must keep bracketing the magnitudes they were derived from: the
/// residual bound below what ScrAP-351's fixture-scale leak leaves over the window
/// (its rises less the largest one), the ceiling above the largest one-time step
/// measured. A compile-time assertion rather than a test, because an edit that
/// inverts either one has made the gate decorative and should not build.
const _: () = assert!(
    GROWTH_BOUNDS.residual_bytes < FIXTURE_LEAK_PER_RENDER * (SAMPLE_COUNT - WARMUP - 2) as u64
);
const _: () = assert!(GROWTH_BOUNDS.total_bytes > 12_600_000);

/// Warm-up **renders** discarded before a render series is judged. Windows measured its
/// entire 1.09 MB of warm-up arriving at iteration 2; three covers that and
/// the GTK icon-cache / font first-paint on the other seats.
///
/// **This is the figure for a one-shot operation, and it does not transfer to a
/// tick-driven one.** A render either has warmed up or has not; a playing animation
/// reaches steady state over a whole cycle, because its decoder, its canvas and the
/// frame clock all settle at their own pace. The playback gate therefore states its own
/// (`memgate::playback`), which is why [`assert_bounded`] takes the warm-up rather than
/// reading this constant — the macOS seat measured four failures in ten runs of one
/// unchanged build there, every failing series rising and then going byte-identical for
/// its last 20–28 samples, which is warm-up still finishing rather than a climb (a leak
/// is still climbing at the last sample).
pub(crate) const WARMUP: usize = 3;

/// Samples collected *including* warm-up. After discarding [`WARMUP`] this
/// leaves ten readings — nine rises, so a climb of `x` per render shows up as
/// `8x` of residual growth while one allocation of any size shows up as none.
pub(crate) const SAMPLE_COUNT: usize = WARMUP + 10;

/// Samples for the uncached-decode gates (TDD 6.9), including warm-up: twenty after it,
/// so a per-decode leak leaves `18x` of residual rather than `8x`. Their smallest
/// mutation (retaining every animated-WebP decode) left ~3.35 MB over ten samples, only
/// ~6% past the residual bound; over twenty it MEASURED 9.35 MB, about 3x the bound
/// (operator decision, 2026-10-02). A decode is cheap, so the longer window costs
/// seconds.
#[cfg(all(test, feature = "memory-gates"))]
pub(crate) const UNCACHED_SAMPLE_COUNT: usize = WARMUP + 20;

/// Set by the `gtk_suite` child process before it runs its cases, and by nothing else.
///
/// **The instrument is process-wide and the failure direction is the reassuring one.**
/// `current()` reads the whole process's footprint, so a foreign allocation made while
/// a series is being collected lands in that series. If it arrives in the first half it
/// raises the baseline and HIDES the growth — the gate then passes on a real leak,
/// which is the reading nobody investigates.
///
/// The memgate bodies register with both harnesses. The `harness = false` suite child
/// runs one case at a time on its main thread with nothing beside it. The ordinary
/// libtest binary runs every plain `#[test]` of the library on other threads while a
/// series is sampled, and none of them could be made to take a lock. A lock among the
/// memgate bodies alone excluded nothing that contends: they were already serial under
/// both harnesses (`#[gtk::test]` funnels every body onto one worker,
/// GTK4Rs/AP-159). So the bodies refuse to measure anywhere but the suite child, and
/// say so, instead.
///
/// Same shape, and the same cause, as the counting allocator in richimg's
/// oversized-allocation targets: contention on a shared instrument presents as a null
/// reading, and here the null reading is "no growth".
#[cfg(all(test, feature = "memory-gates"))]
pub(crate) static IN_SUITE_CHILD: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Claim the instrument for this test's whole body, or `None` — after printing a
/// `SKIPPED` line naming `rubric` — when this process is not the suite child, where a
/// measurement would be unsound. Callers return on `None`.
///
/// Called at the top of the body rather than around the sampling loop, deliberately:
/// building the window, decoding the fixture and pumping the loop all allocate, and a
/// series whose BASELINE was taken while another test was allocating is as wrong as
/// one whose samples were. The measured region is the test.
///
/// **On Linux it also stops the kernel collapsing this process's memory into huge
/// pages**, because that moves the reading with no allocation at all. Under
/// `transparent_hugepage=always` (the GitHub Linux runner; development hosts
/// default to `madvise`), `khugepaged` scans in the background and collapses any
/// 2 MB-aligned stretch of heap with even one resident page into a whole huge
/// page, filling the untouched remainder. Measured on the runner mid-series:
/// VmRSS +12.3 MB in one sample, the heap's `AnonHugePages` 0 → 16 MB, malloc's
/// in-use bytes +16 — a step at a different sample each run, owned by no code in
/// this process. `PR_SET_THP_DISABLE` removes the process from `khugepaged`'s
/// scan, so what remains in the series is growth the program made (GEP-95).
#[cfg(all(test, feature = "memory-gates"))]
#[must_use = "the instrument is only claimed while the guard is alive"]
pub(crate) fn measuring(rubric: &str) -> Option<MeasurementGuard> {
    if !IN_SUITE_CHILD.load(std::sync::atomic::Ordering::SeqCst) {
        println!(
            "SKIPPED [TDD {rubric}]: the footprint instrument is process-wide; run it via \
             `--test gtk_suite memgate` (scripts/run-memory-gates.sh)"
        );
        return None;
    }
    if !proceed_given_screen(rubric, "at the start") {
        return None;
    }
    crate::platform::set_huge_page_collapse_disabled(true);
    Some(MeasurementGuard { _private: () })
}

/// What the login session says about the screen, three ways because "no lock key" and "the
/// read failed" mean opposite things (see `platform::mac::process::session_screen_lock`).
#[cfg(all(test, feature = "memory-gates"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScreenState {
    Unlocked,
    Locked,
    /// The session dictionary could not be read. Not evidence of a lock, so not a reason to
    /// skip; the series is measured and the gate's own bounds judge it.
    Unreadable,
}

/// Fold the platform's three-way read into a [`ScreenState`]: the key is ABSENT on an
/// unlocked screen (it is not `false`), and a read that failed is neither.
#[cfg(all(test, feature = "memory-gates"))]
pub(crate) fn screen_state_from(session: Option<Option<bool>>) -> ScreenState {
    match session {
        None => ScreenState::Unreadable,
        Some(Some(true)) => ScreenState::Locked,
        Some(Some(false) | None) => ScreenState::Unlocked,
    }
}

/// What a gate does about the screen's state.
#[cfg(all(test, feature = "memory-gates"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LockVerdict {
    Measure,
    /// Not a verdict on the application: say so and return.
    Skip,
    /// Under CI a locked screen is a broken runner, and skipping would hide the gate for
    /// good if the runner were locked on every run.
    Refuse,
}

#[cfg(all(test, feature = "memory-gates"))]
pub(crate) fn lock_verdict(state: ScreenState, under_ci: bool) -> LockVerdict {
    match (state, under_ci) {
        (ScreenState::Locked, true) => LockVerdict::Refuse,
        (ScreenState::Locked, false) => LockVerdict::Skip,
        (ScreenState::Unlocked | ScreenState::Unreadable, _) => LockVerdict::Measure,
    }
}

/// Why a footprint series cannot be trusted on a locked screen.
///
/// MEASURED, macOS seat, release build: with the screen locked every window open/close cycle
/// leaves ~10-20 MB behind (the series climbs 21 MB a cycle with no plateau; `footprint`
/// reports it as "Owned physical footprint (unmapped)" and IOSurface, malloc barely moves),
/// and draining the loop six more seconds frees none of it. The widget-finalization half of
/// the same gate passes in that state, so the application released everything and the
/// window server did not. The same run on an unlocked screen leaves 0.1-0.5 MB.
#[cfg(all(test, feature = "memory-gates"))]
const LOCKED_CAUSE: &str =
    "the screen is locked, and the window server then keeps a closed window's \
    backing surfaces (MEASURED: ~10-20 MB per window cycle that nothing frees), so a footprint \
    series reads as a leak the application does not have";

#[cfg(all(test, feature = "memory-gates"))]
fn under_ci() -> bool {
    std::env::var_os("CI").is_some_and(|v| !v.is_empty())
}

/// Whether a gate may measure (or judge) now, given the screen. Prints the `SKIPPED` line
/// naming the cause when it may not, and panics under CI. `when` says which of the two
/// checks this is: a lock can arrive between them.
#[cfg(all(test, feature = "memory-gates"))]
fn proceed_given_screen(rubric: &str, when: &str) -> bool {
    let state = screen_state_from(crate::platform::session_screen_lock());
    match lock_verdict(state, under_ci()) {
        LockVerdict::Measure => true,
        LockVerdict::Skip => {
            println!(
                "SKIPPED [TDD {rubric}]: {LOCKED_CAUSE} (checked {when}); unlock the screen and \
                 re-run"
            );
            false
        }
        LockVerdict::Refuse => panic!(
            "TDD {rubric}: {LOCKED_CAUSE} (checked {when}). Under CI a skip would hide this gate \
             for good, so it fails instead: the runner's session must stay unlocked"
        ),
    }
}

/// What to print when a series fails, on macOS: the screen state and the process's own
/// memory categories, so a red names WHERE the growth sits (heap, or the window server's
/// surfaces) rather than only how much. Best-effort: a missing `footprint` tool prints nothing.
#[cfg(all(test, feature = "memory-gates", target_os = "macos"))]
fn print_failure_context() {
    println!(
        "[memgate context] screen={:?} CI={}",
        screen_state_from(crate::platform::session_screen_lock()),
        under_ci()
    );
    if let Ok(out) = std::process::Command::new("footprint")
        .arg(std::process::id().to_string())
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        println!(
            "[memgate context] footprint:\n{}",
            text.lines().take(24).collect::<Vec<_>>().join("\n")
        );
    }
}

#[cfg(all(test, feature = "memory-gates", not(target_os = "macos")))]
fn print_failure_context() {}

/// Idempotent and process-wide; a refusal is fatal rather than ignored, because
/// a series measured with the collapse still armed is the one that looks like a
/// leak on one host only.
#[cfg(all(test, feature = "memory-gates"))]
pub(crate) struct MeasurementGuard {
    _private: (),
}

/// A test that installs process-global state restores it (POLICY § Unit tests): the
/// huge-page opt-out ends with the measurement, so a case run after a gate in the same
/// process sees the kernel's ordinary behaviour.
#[cfg(all(test, feature = "memory-gates"))]
impl Drop for MeasurementGuard {
    fn drop(&mut self) {
        crate::platform::set_huge_page_collapse_disabled(false);
    }
}

/// Judge a sampled series against this platform's [`GROWTH_BOUNDS`], discarding
/// `warmup` leading samples and naming `rubric` in both the log line and the panic.
///
/// Every gate goes through here rather than reading the BOUNDS itself: six call sites
/// each carrying their own bounds is six places one can be mis-edited, and the log line
/// is what a later failure gets compared against. The **warm-up** is the one thing a
/// gate does state for itself, because it is a property of the operation being sampled
/// rather than of the platform — see [`WARMUP`]. Passing it explicitly is what stopped
/// a render's figure from silently governing an animation's.
#[cfg(all(test, feature = "memory-gates"))]
pub(crate) fn assert_bounded(rubric: &str, warmup: usize, samples: &[u64]) {
    // Checked again here, not only at the start: the screen can lock DURING a series, and a
    // series sampled partly on a locked screen is as unreadable as one taken wholly on it.
    if !proceed_given_screen(rubric, "before judging") {
        return;
    }
    println!(
        "[memgate {rubric}] {}",
        super::growth::describe(samples, warmup, GROWTH_BOUNDS)
    );
    super::growth::assert_no_growth(samples, warmup, GROWTH_BOUNDS).unwrap_or_else(|err| {
        print_failure_context();
        panic!("TDD {rubric}: {err}")
    });
}

/// Current process footprint in bytes, or `None` if this platform's sampler
/// could not read it. A `None` is a broken instrument, not a zero — the
/// caller must refuse rather than treat it as a flat series.
pub(crate) fn current() -> Option<u64> {
    crate::platform::process_footprint_bytes()
}

#[cfg(test)]
mod tests {
    use super::current;

    #[test]
    fn current_returns_a_nonzero_reading_on_this_host() {
        let n = current().expect("footprint sampler must work on a supported host");
        assert!(
            n > 0,
            "a zero footprint is a dark instrument, not a reading"
        );
    }

    #[cfg(feature = "memory-gates")]
    #[test]
    fn the_session_read_tells_a_missing_lock_key_from_a_failed_read() {
        use super::{screen_state_from, ScreenState};
        // An unlocked screen has NO key: absence is "unlocked", never "unreadable".
        assert_eq!(screen_state_from(Some(None)), ScreenState::Unlocked);
        assert_eq!(screen_state_from(Some(Some(false))), ScreenState::Unlocked);
        assert_eq!(screen_state_from(Some(Some(true))), ScreenState::Locked);
        // A dictionary that could not be read is not evidence of a lock.
        assert_eq!(screen_state_from(None), ScreenState::Unreadable);
    }

    #[cfg(feature = "memory-gates")]
    #[test]
    fn a_locked_screen_skips_locally_and_refuses_under_ci() {
        use super::{lock_verdict, LockVerdict::*, ScreenState::*};
        assert_eq!(lock_verdict(Locked, false), Skip);
        assert_eq!(lock_verdict(Locked, true), Refuse);
        for under_ci in [false, true] {
            assert_eq!(lock_verdict(Unlocked, under_ci), Measure);
            assert_eq!(lock_verdict(Unreadable, under_ci), Measure);
        }
    }

    #[cfg(all(feature = "memory-gates", target_os = "macos"))]
    #[test]
    fn the_session_dictionary_is_readable_on_this_host() {
        // Not asserting lock or unlock (the host decides that): asserting the READ works,
        // since an unreadable dictionary would make every gate measure on a locked screen.
        assert!(
            crate::platform::session_screen_lock().is_some(),
            "CGSessionCopyCurrentDictionary returned nothing: no login session to read"
        );
    }

    #[test]
    fn sample_shape_and_bounds_are_the_stated_constants() {
        assert_eq!(super::SAMPLE_COUNT, super::WARMUP + 10);
        assert_eq!(super::GROWTH_BOUNDS.residual_bytes, 3 * 1024 * 1024);
        assert_eq!(super::GROWTH_BOUNDS.total_bytes, 24 * 1024 * 1024);
    }
}
