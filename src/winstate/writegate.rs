//! One-writer-at-a-time gate for a document, and the pass that proves you hold it.
//!
//! # The hazard
//!
//! A document write is dispatched to GLib's I/O thread pool and its completion
//! comes back later, so the GTK main loop runs while it is out — which means a
//! second Save can start before the first has landed. Two writes to one path are
//! not merely wasteful: their renames and their completion callbacks are ordered
//! independently by the pool (which explicitly re-sorts its queue,
//! `gtask.c:2199`), so the last text to reach disk and the last text recorded as
//! the clean baseline can be *different texts*. The application then believes it
//! saved something it did not — the C1 failure the whole save path exists to
//! prevent, arriving precisely on the slow filesystem the asynchronous write was
//! introduced for.
//!
//! # Why a type rather than a `Cell<bool>` and a rule
//!
//! The gate has to be released on *every* exit from the write, including the error
//! paths and any early return a future edit adds. A raw flag makes that a thing to
//! remember, and forgetting it is silent and permanent: the flag stays set, Save
//! stops working for that document for the rest of the session, and nothing logs,
//! warns or fails. Releasing on `Drop` makes the correct behaviour the only
//! behaviour — the same reasoning, and the same shape, as
//! [`SelfDeleteGuard`](super::SelfDeleteGuard), which was promoted out of a bare
//! `Cell` for exactly this reason.
//!
//! Being a plain data type with no GTK dependency, it is also unit-tested directly
//! rather than only through a live `TabState`.

use std::cell::Cell;

/// Tracks whether a write of one document is outstanding.
#[derive(Default)]
pub(crate) struct WriteGate {
    busy: Cell<bool>,
}

/// Proof that the holder owns the right to write. Releases the gate when dropped.
///
/// Deliberately carries no data and has no methods: its whole job is to exist for
/// the duration of a write and to be impossible to obtain twice at once.
pub(crate) struct WritePass<'a> {
    gate: &'a WriteGate,
}

impl WriteGate {
    /// Claim the right to write this document, or `None` if a write is already
    /// outstanding.
    ///
    /// A refusal is a **drop**, not a queue. The buffer is still dirty when a
    /// second Save is refused, so Save stays enabled and pressing it again writes
    /// the newest text; queuing would instead commit an intermediate state nobody
    /// asked for. (The crash-recovery snapshot writer faces the same hazard and
    /// coalesces instead — because its writes are unprompted, so no user is waiting
    /// on any particular one. Same premise, different correct answer, which is why
    /// this type does not try to serve both.)
    pub(crate) fn claim(&self) -> Option<WritePass<'_>> {
        if self.busy.replace(true) {
            None
        } else {
            Some(WritePass { gate: self })
        }
    }

    /// Whether a write is currently outstanding, without trying to take it.
    ///
    /// **Was test-only, and the reason it was is still the rule**: the application
    /// never needs to ask in order to *write*, because every such caller either holds
    /// a [`WritePass`] or was refused one, and both of those answers are already the
    /// outcome. Branching on this and then acting a moment later is the check-then-act
    /// race the pass exists to make unrepresentable. **Do not use it to decide whether
    /// to write.**
    ///
    /// **ONE SANCTIONED CALLER**, declared here rather than at the call site so the
    /// exception lives with the rule: `app::open`'s file-monitor closure, classifying a
    /// `Deleted` event that has *already happened* as ours or external
    /// (`SelfDeleteGuard::swallows`). That is not the hazard above and cannot become it
    /// — it decides nothing about a future write, there is no `await` between the read
    /// and the decision, and the monitor closure runs on the same single-threaded main
    /// context as the pass it is asking about, so the answer cannot change underneath
    /// it. A new caller that does not meet all three of those conditions wants
    /// [`claim`](Self::claim), not this.
    ///
    /// **Now enforced by `clippy.toml`'s `disallowed-methods`**, which is what makes the
    /// paragraph above a rule rather than a wish: the sanctioned caller carries the sole
    /// `#[allow]`, and `-D warnings` turns a second one into a build failure. Until that
    /// ban existed this was prose on a `pub(crate)` method — a second caller would have
    /// cost nothing and broken nothing visibly, which is the only kind of rule that gets
    /// broken.
    pub(crate) fn is_busy(&self) -> bool {
        self.busy.get()
    }
}

impl Drop for WritePass<'_> {
    fn drop(&mut self) {
        self.gate.busy.set(false);
    }
}

#[cfg(test)]
// This module's tests are the gate's OWN unit tests: `is_busy` is the observable
// they assert the state machine through, which is the reading the ban's rule
// explicitly permits (assert on it, never branch on it to write).
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn a_second_claim_is_refused_while_the_first_is_held() {
        let gate = WriteGate::default();
        let held = gate.claim().expect("the first claim succeeds");
        assert!(gate.is_busy());
        assert!(
            gate.claim().is_none(),
            "a second write must be refused, not raced: two writes to one path can \
             land in either order, so the newest text on disk and the newest \
             baseline recorded can disagree (C1)"
        );
        drop(held);
        assert!(!gate.is_busy());
        assert!(gate.claim().is_some(), "and the gate reopens afterwards");
    }

    /// The release is the reason this is a type. A write that returns early — an
    /// I/O error, or any future edit's `?` — must still reopen the gate, because a
    /// gate left shut disables Save for that document for the rest of the session
    /// with no error, no warning and no failing test.
    ///
    /// Mutation: replacing the `Drop` impl with an explicit `release()` that this
    /// path forgets to call fails this.
    #[test]
    fn an_early_exit_still_releases_the_gate() {
        let gate = WriteGate::default();
        fn write_that_fails(gate: &WriteGate) -> Result<(), &'static str> {
            let _pass = gate.claim().ok_or("busy")?;
            Err("the write failed")
        }
        assert!(write_that_fails(&gate).is_err());
        assert!(
            !gate.is_busy(),
            "the gate must reopen when the write returns early"
        );
    }

    /// …and a panic in the write is the same case, since unwinding runs `Drop`.
    /// POLICY forbids `panic = "abort"` (the crash-report hook needs the unwind),
    /// so this holds in the shipped binary and not only under test.
    #[test]
    fn a_panic_in_the_write_still_releases_the_gate() {
        let gate = WriteGate::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _pass = gate.claim().expect("claimed");
            panic!("the write panicked");
        }));
        assert!(result.is_err());
        assert!(!gate.is_busy(), "unwinding released the gate");
    }
}
