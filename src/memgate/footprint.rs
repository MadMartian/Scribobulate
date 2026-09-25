//! Process footprint sampler — one function, three `cfg` bodies.
//!
//! The returned number is **not RSS**. Linux reads `VmRSS`, macOS reads
//! `ri_phys_footprint`, Windows reads `WorkingSetSize`. A shared name invites
//! a shared threshold; the bounds a series is judged against live next to this
//! module in [`GROWTH_BOUNDS`], and [`assert_bounded`] is the only thing that
//! reads them.

/// Per-platform bounds for [`super::growth::assert_no_growth`], in bytes.
///
/// The one owner of both numbers, and [`assert_bounded`] the only reader.
///
/// **Both are derived from clean traces, and the run log carries the numbers
/// they were derived from.** `residual_bytes` sits above the growth a clean run
/// leaves unexplained by its single largest allocation, and far below what a
/// per-render climb leaves: ScrAP-351's fixture-scale leak is ~1.05 MB per
/// render, which over a ten-sample window leaves ~9.4 MB of residual — two
/// orders of magnitude above the bound rather than beside it, so no host's churn
/// sits near the decision. `total_bytes` sits above the largest one-time step
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
/// | Windows CI runner | 0.26 MB | 0.53 MB | −0.28 MB |
///
/// **The armed CI row is the one that decided the bound**, and it is the trace
/// this predicate exists for: a ~12 MB one-time step, at a different sample each
/// run, which the half-mean this replaced reported as an 8.65 MB climb. It was
/// never an allocation — `khugepaged` filling heap pages the process already
/// owned (see `measuring()`) — but a step the program did not make is exactly
/// the shape the predicate must pass. Its 0.86 MB of residual is the worst
/// clean reading anywhere, and it sits 2.3x under the bound while the smallest
/// leak the gate must catch — ScrAP-351's ~1.05 MB per render over a ten-sample
/// window — sits 4.5x over it. Windows shows the bound must tolerate a NEGATIVE
/// residual: its footprint falls across the window, which is not growth.
/// A leak arriving in two chunks rather than one is the shape this cannot see;
/// the ceiling below is what bounds it.
pub(crate) const GROWTH_BOUNDS: super::growth::Bounds = super::growth::Bounds {
    residual_bytes: 2 * 1024 * 1024,
    total_bytes: 24 * 1024 * 1024,
};

/// The bounds must keep bracketing the magnitudes they were derived from: the
/// residual bound far below what ScrAP-351's fixture-scale leak leaves over a
/// ten-sample window, the ceiling above the largest one-time step measured. A compile-time assertion rather than a test, because an edit that
/// inverts either one has made the gate decorative and should not build.
const _: () = assert!(GROWTH_BOUNDS.residual_bytes < 9 * 1024 * 1024);
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

/// Serialises every footprint measurement in the process.
///
/// **The instrument is process-wide and the failure direction is the reassuring one.**
/// `current()` reads the whole process's footprint, so a foreign allocation made while
/// a series is being collected lands in that series. If it arrives in the first half it
/// raises the baseline and HIDES the growth — the gate then passes on a real leak,
/// which is the reading nobody investigates.
///
/// That is reachable because these bodies register with both harnesses: the
/// `harness = false` suite runs them one at a time on the main thread, and the ordinary
/// libtest binary runs them in parallel threads. The runner script pins the first; it
/// cannot pin the second, and a claim about a shared instrument that depends on which
/// harness invoked it is not a claim.
///
/// Same shape, and the same cause, as the counting allocator in richimg's
/// oversized-allocation targets: contention on a shared instrument presents as a null
/// reading, and here the null reading is "no growth".
#[cfg(all(test, feature = "memory-gates"))]
static MEASUREMENT: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Claim the instrument for this test's whole body, and release it on drop.
///
/// A guard rather than a wrapper around the sampling loop, deliberately: building the
/// window, decoding the fixture and pumping the loop all allocate, and a series whose
/// BASELINE was taken while another test was allocating is as wrong as one whose
/// samples were. The measured region is the test.
///
/// Poisoning is ignored: a failing assertion inside one series must not turn every
/// later one into a second, misleading failure.
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
pub(crate) fn measuring() -> MeasurementGuard {
    #[cfg(target_os = "linux")]
    disable_huge_page_collapse();
    MeasurementGuard {
        _lock: MEASUREMENT.lock().unwrap_or_else(|e| e.into_inner()),
    }
}

/// Idempotent and process-wide; a refusal is fatal rather than ignored, because
/// a series measured with the collapse still armed is the one that looks like a
/// leak on one host only.
#[cfg(all(test, feature = "memory-gates", target_os = "linux"))]
fn disable_huge_page_collapse() {
    // SAFETY: PR_SET_THP_DISABLE takes one integer flag and no pointers; the
    // trailing arguments are required to be zero.
    let rc = unsafe { libc::prctl(libc::PR_SET_THP_DISABLE, 1, 0, 0, 0) };
    assert_eq!(
        rc,
        0,
        "PR_SET_THP_DISABLE refused: {}",
        std::io::Error::last_os_error()
    );
}

#[cfg(all(test, feature = "memory-gates"))]
pub(crate) struct MeasurementGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
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
    println!(
        "[memgate {rubric}] {}",
        super::growth::describe(samples, warmup, GROWTH_BOUNDS)
    );
    super::growth::assert_no_growth(samples, warmup, GROWTH_BOUNDS)
        .unwrap_or_else(|err| panic!("TDD {rubric}: {err}"));
}

/// Current process footprint in bytes, or `None` if this platform's sampler
/// could not read it. A `None` is a broken instrument, not a zero — the
/// caller must refuse rather than treat it as a flat series.
pub(crate) fn current() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        linux_vmrss_bytes()
    }
    #[cfg(target_os = "macos")]
    {
        macos_phys_footprint()
    }
    #[cfg(windows)]
    {
        crate::platform::win32::process::current_working_set_bytes()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        None
    }
}

#[cfg(target_os = "linux")]
fn linux_vmrss_bytes() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/self/status").ok()?;
    parse_vmrss_kb(&text).map(|kb| kb.saturating_mul(1024))
}

/// Parse `VmRSS:` from a `/proc/<pid>/status` blob. Split from the file read
/// so the grammar is unit-tested with no `/proc`.
#[cfg(target_os = "linux")]
fn parse_vmrss_kb(status: &str) -> Option<u64> {
    for line in status.lines() {
        let Some(rest) = line.strip_prefix("VmRSS:") else {
            continue;
        };
        return rest.split_whitespace().next()?.parse::<u64>().ok();
    }
    None
}

/// macOS physical footprint via `proc_pid_rusage(RUSAGE_INFO_V2)`.
///
/// `RUSAGE_INFO_V2` is the earliest flavour that carries `ri_phys_footprint`.
/// Later flavours add fields this gate does not use.
///
/// ⚠ This number is a high-water of pages the zone still holds, not "bytes
/// currently referenced". macOS malloc keeps freed pages: a 256 MB allocation
/// dropped moved the reading by nothing. Never write a single-shot
/// "allocate, free, assert this came back" against it.
#[cfg(target_os = "macos")]
fn macos_phys_footprint() -> Option<u64> {
    // SAFETY: `rusage_info_v2` is the buffer `RUSAGE_INFO_V2` writes; a zeroed
    // struct is a valid empty starting point, and `getpid` is this process.
    unsafe {
        let mut info: libc::rusage_info_v2 = std::mem::zeroed();
        let rc = libc::proc_pid_rusage(
            libc::getpid(),
            libc::RUSAGE_INFO_V2,
            std::ptr::addr_of_mut!(info) as *mut libc::rusage_info_t,
        );
        if rc == 0 {
            Some(info.ri_phys_footprint)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::current;

    #[cfg(target_os = "linux")]
    #[test]
    fn parse_vmrss_reads_the_kb_field() {
        let blob = "Name:\tfoo\nVmPeak:\t999 kB\nVmRSS:\t  1234 kB\nVmData:\t1 kB\n";
        assert_eq!(super::parse_vmrss_kb(blob), Some(1234));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn parse_vmrss_none_when_the_field_is_absent() {
        assert_eq!(super::parse_vmrss_kb("Name:\tfoo\nVmPeak:\t9 kB\n"), None);
    }

    #[test]
    fn current_returns_a_nonzero_reading_on_this_host() {
        let n = current().expect("footprint sampler must work on a supported host");
        assert!(
            n > 0,
            "a zero footprint is a dark instrument, not a reading"
        );
    }

    #[test]
    fn sample_shape_and_bounds_are_the_stated_constants() {
        assert_eq!(super::SAMPLE_COUNT, super::WARMUP + 10);
        assert_eq!(super::GROWTH_BOUNDS.residual_bytes, 2 * 1024 * 1024);
        assert_eq!(super::GROWTH_BOUNDS.total_bytes, 24 * 1024 * 1024);
    }
}
