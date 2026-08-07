//! Per-tab crash-recovery snapshot state: the debounce bookkeeping, the maximum-latency
//! cap, and the in-flight/coalescing gate.
//!
//! The *decisions* live here as pure functions over plain data so they are unit-testable
//! with no display and no filesystem; the GTK timers and the actual write live in
//! `window/swap.rs`.

use std::cell::Cell;

/// Per-tab snapshot bookkeeping. One of these hangs off each `TabState`.
#[derive(Default)]
pub(crate) struct SwapState {
    /// The pending debounce timer, if one is armed.
    pub(crate) pending: Cell<Option<glib::SourceId>>,
    /// Monotonic deadline (µs, `glib::monotonic_time` domain) by which a snapshot must
    /// happen regardless of continued typing — the maximum-latency cap. `None` when no
    /// edit is outstanding.
    pub(crate) deadline: Cell<Option<i64>>,
    /// Whether a write is currently in flight. GIO does **not** order concurrent
    /// replaces of the same file (GTK4Rs/AP-167), so two in flight could land out of order
    /// and silently resurrect an older buffer state.
    pub(crate) in_flight: Cell<bool>,
    /// Whether a snapshot was requested while one was in flight, to be fired when it
    /// completes. Latest-wins: the flag says *something changed*, never *what*, because
    /// the payload is re-read from the live buffer at write time anyway.
    pub(crate) coalesced: Cell<bool>,
    /// Whether we believe a swap file currently exists for this tab. Lets the
    /// clean-document path skip a `unlink` syscall on every keystroke without weakening
    /// the invariant: it is only ever an optimisation over "delete unconditionally", and
    /// a stale `true` costs one harmless failed delete.
    pub(crate) on_disk: Cell<bool>,
}

/// The idle debounce for a document of `bytes` bytes, in milliseconds.
///
/// **3 s at ordinary document sizes** (the operator's decision): long enough that
/// continuous typing does not queue a write per word, short enough that a crash costs a
/// sentence rather than a session.
///
/// It grows with document size because the snapshot's cost does. A snapshot is a full
/// copy of the buffer plus a full write, and at the project's document ceiling that is
/// tens of megabytes *per debounce* — a flat 3 s there would be a self-inflicted
/// performance defect. The curve is linear between the two anchor points and clamped at
/// both ends.
///
/// Note what the focus-loss flush does to this: because leaving the editor pane commits
/// immediately, the debounce only ever elapses *while the user is still typing into the
/// pane*. It is a typing-burst absorber, not the mechanism that bounds data loss — the
/// cap below is.
pub(crate) fn debounce_ms(bytes: usize) -> u64 {
    /// Debounce at or below [`SMALL_DOCUMENT_BYTES`].
    const MIN_MS: u64 = 3_000;
    /// Debounce at or above [`LARGE_DOCUMENT_BYTES`].
    const MAX_MS: u64 = 30_000;
    /// Below this, a snapshot is cheap enough that only the typing burst matters.
    const SMALL_DOCUMENT_BYTES: usize = 1 << 20; // 1 MiB
    /// At and above this, snapshot cost dominates. Matches the document size ceiling.
    const LARGE_DOCUMENT_BYTES: usize = 64 << 20; // 64 MiB

    if bytes <= SMALL_DOCUMENT_BYTES {
        return MIN_MS;
    }
    if bytes >= LARGE_DOCUMENT_BYTES {
        return MAX_MS;
    }
    let span = (LARGE_DOCUMENT_BYTES - SMALL_DOCUMENT_BYTES) as u64;
    let over = (bytes - SMALL_DOCUMENT_BYTES) as u64;
    MIN_MS + (MAX_MS - MIN_MS) * over / span
}

/// The maximum latency cap, in milliseconds: the longest a document may stay dirty
/// without being snapshotted, however continuously the user types.
///
/// This is the part a naive debounce gets wrong, and it is the number that actually
/// decides what a crash costs. A pure idle debounce never fires for a user typing
/// steadily for ten minutes — precisely the user with the most to lose.
pub(crate) const MAX_LATENCY_MS: u64 = 30_000;

/// How long to wait before the next snapshot attempt, given the document size and the
/// outstanding deadline.
///
/// Returns the idle debounce, shortened where necessary so the deadline is not
/// overrun — `0` meaning "the cap has already expired, snapshot now".
///
/// Times are in the `glib::monotonic_time` domain (microseconds) and the arithmetic is
/// saturating: a wall-clock or accumulated-delta bound would be wrong for the usual
/// reasons (GTK4Rs/AP-122), and a monotonic deadline compared with saturating
/// subtraction cannot go negative or wrap.
pub(crate) fn next_delay_ms(bytes: usize, now_us: i64, deadline_us: Option<i64>) -> u64 {
    let debounce = debounce_ms(bytes);
    match deadline_us {
        None => debounce,
        Some(deadline) => {
            let remaining_ms = (deadline.saturating_sub(now_us) / 1_000).max(0) as u64;
            debounce.min(remaining_ms)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{debounce_ms, next_delay_ms, MAX_LATENCY_MS};

    const MIB: usize = 1 << 20;

    #[test]
    fn an_ordinary_document_uses_the_three_second_debounce() {
        assert_eq!(debounce_ms(0), 3_000);
        assert_eq!(debounce_ms(4_000), 3_000);
        assert_eq!(debounce_ms(MIB), 3_000);
    }

    #[test]
    fn a_document_at_the_size_ceiling_uses_the_long_debounce() {
        assert_eq!(debounce_ms(64 * MIB), 30_000);
        assert_eq!(
            debounce_ms(usize::MAX),
            30_000,
            "clamped, never overflowing"
        );
    }

    #[test]
    fn the_debounce_grows_monotonically_with_size() {
        let mut previous = 0;
        for mib in [0usize, 1, 2, 8, 16, 32, 64, 128] {
            let ms = debounce_ms(mib * MIB);
            assert!(
                ms >= previous,
                "debounce fell from {previous} to {ms} at {mib} MiB"
            );
            previous = ms;
        }
    }

    #[test]
    fn with_no_deadline_the_delay_is_just_the_debounce() {
        assert_eq!(next_delay_ms(0, 1_000_000, None), 3_000);
    }

    #[test]
    fn a_far_deadline_does_not_shorten_the_debounce() {
        let now = 1_000_000;
        let deadline = now + (MAX_LATENCY_MS as i64) * 1_000;
        assert_eq!(next_delay_ms(0, now, Some(deadline)), 3_000);
    }

    #[test]
    fn a_near_deadline_shortens_the_debounce() {
        // The continuous-typing case: each keystroke re-arms the 3 s debounce, but the
        // cap must still land on time.
        let now = 1_000_000;
        let deadline = now + 500 * 1_000; // 500 ms away
        assert_eq!(next_delay_ms(0, now, Some(deadline)), 500);
    }

    #[test]
    fn an_expired_deadline_asks_for_an_immediate_snapshot() {
        let now = 10_000_000;
        assert_eq!(next_delay_ms(0, now, Some(now - 5_000_000)), 0);
    }

    #[test]
    fn a_deadline_far_in_the_past_does_not_wrap_or_panic() {
        // Saturating arithmetic: the failure this guards is a negative duration read as
        // an enormous unsigned one, which would silently disable the cap forever.
        assert_eq!(next_delay_ms(0, i64::MAX, Some(i64::MIN)), 0);
    }
}
