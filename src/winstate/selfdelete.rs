//! The atomic-save round-trip guard.

use std::cell::Cell;

/// The atomic-save round-trip guard's arm/consume/clear protocol (GTK4Rs/AP-62),
/// extracted as a plain, GTK-free data type (QA round-2
/// R2-2) — its three methods are the ONLY way to touch the underlying flag,
/// enforcing at the type level what used to be a convention spread across
/// five call sites in two modules (`window/save.rs`, `app.rs`) with no
/// static link between them (the original GTK4Rs/AP-1 structural root cause). Being
/// plain data with no GTK dependency, it is unit-tested directly here rather
/// than only reachable through a live, `gtk::init()`-requiring `TabState`.
#[derive(Default)]
pub(crate) struct SelfDeleteGuard(Cell<bool>);

impl SelfDeleteGuard {
    /// Arm the guard immediately before the tab's own `write_atomic` rename.
    pub(crate) fn arm(&self) {
        self.0.set(true);
    }

    /// Disarm WITHOUT treating it as a consumed self-delete — for when the
    /// write that would have armed it never actually happened (a failed
    /// `write_atomic`), when a monitor is freshly (re)pointed and can have
    /// no legitimate pending self-`Deleted` to swallow (the M1 fix), or when
    /// a `Changed`/`Created` event proves no separate `Deleted` is coming.
    pub(crate) fn disarm(&self) {
        self.0.set(false);
    }

    /// Consume the guard: `true` means a `Deleted` event just observed was
    /// our own self-triggered rename (swallow it); `false` means it's a
    /// genuine external deletion the caller should surface. Always leaves
    /// the guard disarmed afterward, so it can only ever swallow one event
    /// per arm.
    pub(crate) fn consume(&self) -> bool {
        self.0.replace(false)
    }

    /// Does this `Deleted` event belong to our own save's rename?
    ///
    /// **The one-shot flag is not enough, because one save is not one `Deleted`.**
    /// MEASURED on the same `write_atomic` rename: Linux delivers `Deleted, Created,
    /// ChangesDoneHint`, while the **Win32 backend delivers TWO `Deleted`s**, 25 µs
    /// apart, for a single `MoveFileEx` replace-existing. With [`consume`](Self::consume)
    /// alone the second one found the guard already disarmed, took the
    /// genuine-external-deletion branch in full, and left the user staring at
    /// "File deleted on disk — save to restore it" over a file that was present and
    /// correct. The count is a property of the platform's monitor backend, so no fixed
    /// number is safe to assume.
    ///
    /// `write_in_flight` is the fallback: while this tab holds its own `WritePass`, a
    /// `Deleted` for its own file is ours by construction, whatever its ordinal.
    ///
    /// **The short-circuit order is load-bearing and must not be flipped.** `consume()`
    /// runs FIRST so today's semantics are preserved everywhere — in particular on
    /// **macOS, which reports `DELETED` and nothing else** (kqueue cannot recover the new
    /// name), where the lone event *must* consume the flag or it stays armed and swallows
    /// the next genuine external delete (GTK4Rs/AP-62). The `write_in_flight` clause can
    /// therefore only ever catch a SECOND-or-later `Deleted` arriving during our own
    /// in-flight write, which is exactly the Windows case and nothing else. Swallowing a
    /// genuinely external delete inside that window is harmless: our own write re-creates
    /// the file microseconds later.
    ///
    /// **Known limitation, stated rather than discovered:** this covers only events that
    /// arrive while the pass is held. Linux's arrive ~100 ms *after* the write completes,
    /// so there the one-shot flag still does the work and this clause never fires — and a
    /// backend that delivered two `Deleted`s after release would defeat both.
    pub(crate) fn swallows(&self, write_in_flight: bool) -> Option<SwallowReason> {
        if self.consume() {
            return Some(SwallowReason::OwnArmedWrite);
        }
        if write_in_flight {
            return Some(SwallowReason::WriteInFlight);
        }
        None
    }
}

/// Why a `Deleted` event was suppressed.
///
/// Returned rather than a bare `bool` because the two arms are not equivalent and the
/// difference is invisible in a log otherwise. `OwnArmedWrite` consumes a flag this
/// application set, so the event is certainly ours. `WriteInFlight` is a FALLBACK for the
/// Windows surplus event, and it is the one cell where a genuinely external deletion can be
/// suppressed — deliberately, because our own write re-creates the file microseconds later,
/// but it is the cell a reader debugging a "my file vanished and nothing said so" report
/// needs to be able to see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SwallowReason {
    /// A flag this application armed before its own write.
    OwnArmedWrite,
    /// A write was in progress; the event is presumed to be its surplus.
    WriteInFlight,
}

#[cfg(test)]
mod tests {
    use crate::winstate::*;

    #[test]
    fn self_delete_guard_starts_disarmed() {
        let g = SelfDeleteGuard::default();
        assert!(!g.consume());
    }

    #[test]
    fn self_delete_guard_arm_then_consume_returns_true_once() {
        let g = SelfDeleteGuard::default();
        g.arm();
        assert!(g.consume());
        // Consuming disarms — a second consume without re-arming is a
        // genuine external event, not a swallowed self-write.
        assert!(!g.consume());
    }

    /// The Windows sequence, MEASURED: one arm, then TWO `Deleted`s during the write.
    /// Both must be swallowed. Mutation guard: drop the `|| write_in_flight` and the
    /// second assertion fails, which is the shipped bug.
    #[test]
    fn both_deletes_of_a_two_event_rename_are_swallowed_while_the_write_is_in_flight() {
        let g = SelfDeleteGuard::default();
        g.arm();
        assert!(
            g.swallows(true).is_some(),
            "first Deleted: consumed by the one-shot flag"
        );
        assert!(
            g.swallows(true).is_some(),
            "second Deleted of the SAME rename must not read as an external deletion"
        );
    }

    /// The macOS path: a lone `DELETED` with no write in flight by the time it lands
    /// must still consume the flag, or the guard stays armed and eats the next genuine
    /// external delete.
    #[test]
    fn a_lone_delete_consumes_the_flag_even_with_no_write_in_flight() {
        let g = SelfDeleteGuard::default();
        g.arm();
        assert!(g.swallows(false).is_some());
        assert!(
            g.swallows(false).is_none(),
            "the flag must not survive the event it explained"
        );
    }

    /// **QA M23/M67 — the one cell where a GENUINELY EXTERNAL deletion is suppressed.**
    ///
    /// Unarmed guard, write in flight: nothing this application armed explains the event,
    /// and it is swallowed anyway on the presumption that it is the Windows surplus. That
    /// is the deliberate trade the doc above describes — our own write re-creates the file
    /// microseconds later — but it was the only cell of the matrix with no test, and the
    /// two arms were indistinguishable from outside because `swallows` returned a bool.
    ///
    /// It now names WHICH arm fired, which is what makes the suppression visible in a log
    /// to someone debugging "my file vanished and nothing said so".
    #[test]
    fn a_write_in_flight_swallows_an_unexplained_delete_and_says_which_arm_did_it() {
        let g = SelfDeleteGuard::default();
        assert_eq!(
            g.swallows(true),
            Some(SwallowReason::WriteInFlight),
            "the fallback arm must fire, and be distinguishable from an armed self-delete"
        );

        let armed = SelfDeleteGuard::default();
        armed.arm();
        assert_eq!(
            armed.swallows(true),
            Some(SwallowReason::OwnArmedWrite),
            "an armed flag is consumed FIRST — the short-circuit order is load-bearing"
        );
    }

    /// A genuine external deletion, with nothing armed and nothing being written, still
    /// surfaces — the fallback must not become a blanket suppressor.
    #[test]
    fn an_unarmed_guard_with_no_write_in_flight_surfaces_the_deletion() {
        let g = SelfDeleteGuard::default();
        assert!(g.swallows(false).is_none());
    }

    #[test]
    fn self_delete_guard_disarm_after_arm_prevents_consume() {
        let g = SelfDeleteGuard::default();
        g.arm();
        g.disarm();
        assert!(!g.consume());
    }
}
