//! A per-document generation counter, and the ticket that says which generation a
//! deferred operation was started against.
//!
//! # The hazard
//!
//! Document reads and writes leave the main thread, so the GTK main loop runs while
//! they are out and **several operations on one document can be in flight at once**.
//! GLib's I/O pool orders neither their completions nor their effects — it explicitly
//! re-sorts its queue (`gtask.c:2199`) — so an operation that finishes second may have
//! started first, and its answer describes a document that no longer exists.
//!
//! Guarding read-against-read is the obvious half and the cheap one. The half that
//! actually loses work is **read-against-write**:
//!
//! 1. A reload (or the live-reload watcher's re-read) starts and its read goes out.
//! 2. The user presses Save. The buffer is written, and `saved_baseline` becomes the
//!    text that is now on disk.
//! 3. The reload's read comes back carrying *pre-save* content, replaces the buffer
//!    with it, and records it as the clean baseline.
//!
//! The tab now reads **clean** while its buffer differs from its file, the text the
//! user just saved is gone from the screen, and the next save writes the stale content
//! back over the good one. Nothing errors, and the dirty indicator — the one surface a
//! user would check — actively says everything is fine.
//!
//! # The rule this type exists to enforce
//!
//! **Anything that changes a document's content or its baseline bumps the epoch;
//! anything that will apply a deferred result checks its ticket first.** One counter
//! gives both properties at once:
//!
//! - a *newer read* supersedes an older one, because a read takes its ticket by
//!   bumping — "I am the newest reader of this document";
//! - a *mutation* supersedes every in-flight read, because it bumps too.
//!
//! One reader is not superseded by a newer one: an explicit File ▸ Reload. A live-reload
//! re-read starting while its read is out stands aside and runs after it instead
//! ([`DocEpoch::claim_explicit`]), because superseding it drops the gesture the user is
//! waiting on and puts the watcher's decision in its place.
//!
//! A write never checks a ticket: it produced the bytes on disk, so its own baseline
//! update is always the truth and must always land. Only *readers* can be superseded.

use std::cell::Cell;

/// Which generation of a document a deferred operation was started against.
///
/// Deliberately opaque and `Copy`: it is only ever obtained from [`DocEpoch::claim`]
/// and only ever handed back to [`DocEpoch::is_current`] on the same document, so
/// there is no arithmetic for a call site to get subtly wrong and no way to compare a
/// ticket against a counter it did not come from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct DocTicket(u64);

/// A document's content generation.
#[derive(Default)]
pub(crate) struct DocEpoch {
    current: Cell<u64>,
    /// The ticket of the read an explicit File ▸ Reload has out, if any.
    explicit: Cell<Option<DocTicket>>,
    /// A background re-read that stood aside for that reload, to run once it lands:
    /// `Some(settled)`, where `settled` is true if any that stood aside was the settle
    /// re-read (`window::reload`'s `check_disk`).
    yielded: Cell<Option<bool>>,
}

impl DocEpoch {
    /// Start a deferred read of this document, superseding any read already out.
    ///
    /// Returns the ticket to check on completion. Claiming bumps the epoch, which is
    /// what makes the newest reader win: an older read's ticket stops being current
    /// the moment a newer one starts.
    pub(crate) fn claim(&self) -> DocTicket {
        let next = self.current.get().wrapping_add(1);
        self.current.set(next);
        DocTicket(next)
    }

    /// Announce that the document's content or baseline has changed.
    ///
    /// Every path that mutates either — a completed save, an applied reload, an
    /// applied crash-recovery snapshot — calls this, and that is the whole contract.
    /// A mutation that forgets to leaves an in-flight read believing its stale answer
    /// still describes the document.
    pub(crate) fn bump(&self) {
        self.current.set(self.current.get().wrapping_add(1));
    }

    /// Whether `ticket` still describes this document.
    ///
    /// `false` means something happened while the operation was out and its result
    /// must be **discarded, not merged**: it is an answer about a document state that
    /// no longer exists, and there is no way to tell from the answer itself.
    pub(crate) fn is_current(&self, ticket: DocTicket) -> bool {
        self.current.get() == ticket.0
    }

    /// Start the read of an explicit File ▸ Reload — the one read a user is waiting on.
    ///
    /// It supersedes whatever read is already out, as [`claim`](Self::claim) does, and
    /// until it lands, background re-reads [`yield_to_explicit`](Self::yield_to_explicit)
    /// instead of claiming over it. Without that, a live-reload re-read starting while
    /// the reload's read was out took the newer ticket, the reload's answer was discarded
    /// as superseded, and the re-read decided as the watcher does: over a dirty buffer,
    /// or a document lost and refilled, that is the conflict prompt again, so the
    /// gesture was silently dropped (TDD 5.3). MEASURED on Windows, where the folder poll
    /// re-reads a deleted document's returned file right after re-arming its monitor.
    pub(crate) fn claim_explicit(&self) -> DocTicket {
        let ticket = self.claim();
        self.explicit.set(Some(ticket));
        ticket
    }

    /// Whether an explicit reload's read is out and still current, so a background
    /// re-read must stand aside for it. One a mutation has superseded is doomed already,
    /// and stands in nobody's way.
    pub(crate) fn explicit_outstanding(&self) -> bool {
        self.explicit.get().is_some_and(|t| self.is_current(t))
    }

    /// A background re-read stood aside for the outstanding explicit reload: record it,
    /// once however many do, to run when that reload lands.
    pub(crate) fn yield_to_explicit(&self, settled: bool) {
        self.yielded
            .set(Some(self.yielded.get().unwrap_or(false) || settled));
    }

    /// The explicit reload holding `ticket` came back, whatever it then does with its
    /// answer. Returns the re-read that stood aside for it, which the caller runs after
    /// applying its own answer: the change that prompted it may postdate the reload's
    /// read. A newer explicit reload still out keeps that re-read for itself.
    pub(crate) fn finish_explicit(&self, ticket: DocTicket) -> Option<bool> {
        match self.explicit.get() {
            Some(held) if held != ticket => None,
            _ => {
                self.explicit.set(None);
                self.yielded.take()
            }
        }
    }
}

/// How many times **this application** has written the document to disk.
///
/// A second, deliberately independent counter from [`DocEpoch`], and the difference
/// is the whole point. `DocEpoch` is bumped by every mutation *and claimed by every
/// reader*, including the live-reload watcher's — which fires per filesystem event,
/// and on a filesystem GIO polls rather than watches, faster than a slow read
/// completes. A guard that re-issues itself whenever `DocEpoch` has moved therefore
/// never observes a quiet moment and the user's Save silently never happens; that was
/// measured, and `window/save.rs` carries the note. This counter moves **only when a
/// save of ours lands**, which no external event source can drive.
///
/// # What it is for
///
/// The save guard reads the file, then compares what it read against
/// `saved_baseline` — but it reads the baseline at *decision* time, which is after
/// its own read came back. If one of our own saves completed while that read was out,
/// the two describe different moments: the bytes are pre-write and the baseline is
/// post-write. They differ, nothing external touched the file, and the user is asked
/// whether to overwrite changes that are their own.
///
/// Observing this counter before the read and re-checking it after is what tells the
/// guard its answer predates its own baseline. The remedy is to read again, never to
/// assume safety: an external writer could have moved in the same window.
#[derive(Default)]
pub(crate) struct WriteEpoch {
    current: Cell<u64>,
}

/// Which generation of *our own writes* an observation was taken against.
///
/// Opaque and `Copy` for the same reason [`DocTicket`] is. Note the asymmetry with
/// that type: taking one of these does **not** bump. An observer here is not claiming
/// to supersede anybody — the save guard must never supersede a reader, only notice
/// that it has been overtaken.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct WriteMark(u64);

impl WriteEpoch {
    /// Note the document's current write generation, superseding nothing.
    pub(crate) fn observe(&self) -> WriteMark {
        WriteMark(self.current.get())
    }

    /// Announce that a save of ours reached disk and updated the baseline.
    pub(crate) fn bump(&self) {
        self.current.set(self.current.get().wrapping_add(1));
    }

    /// Whether no save of ours has landed since `mark` was taken.
    pub(crate) fn is_current(&self, mark: WriteMark) -> bool {
        self.current.get() == mark.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ticket_is_current_until_something_happens() {
        let epoch = DocEpoch::default();
        let ticket = epoch.claim();
        assert!(epoch.is_current(ticket));
        epoch.bump();
        assert!(!epoch.is_current(ticket));
    }

    #[test]
    fn a_newer_read_supersedes_an_older_one() {
        let epoch = DocEpoch::default();
        let first = epoch.claim();
        let second = epoch.claim();
        assert!(
            !epoch.is_current(first),
            "the older read must lose: applying it after the newer one puts stale \
             content in the buffer and records it as the clean baseline"
        );
        assert!(epoch.is_current(second));
    }

    /// The read-against-write case, which is the one that loses the user's work.
    ///
    /// Mutation: removing the `bump()` from the save's completion in
    /// `window/save.rs::save_window` makes this pass while the application silently
    /// reverts a saved document.
    #[test]
    fn a_write_supersedes_a_read_that_was_already_in_flight() {
        let epoch = DocEpoch::default();
        let reload = epoch.claim(); // a reload's read goes out
        epoch.bump(); // …and a save lands while it is out
        assert!(
            !epoch.is_current(reload),
            "a reload that started before a save must not apply after it — it carries \
             pre-save content, and applying it both wipes the saved text from the \
             buffer and records that stale text as clean"
        );
    }

    /// Two independent documents never invalidate each other: the counter is per-tab,
    /// and a ticket is only ever checked against the epoch it came from.
    /// TDD 5.3: a background re-read starting while File ▸ Reload's read is out stands
    /// aside rather than superseding it, and runs once the reload lands.
    #[test]
    fn an_explicit_reload_is_not_superseded_by_a_background_re_read() {
        let epoch = DocEpoch::default();
        let reload = epoch.claim_explicit();
        assert!(
            epoch.explicit_outstanding(),
            "a background re-read must yield"
        );
        epoch.yield_to_explicit(false);
        epoch.yield_to_explicit(true);
        epoch.yield_to_explicit(false);
        assert!(epoch.is_current(reload), "standing aside claims nothing");
        assert_eq!(
            epoch.finish_explicit(reload),
            Some(true),
            "the re-reads that stood aside run once, and the settle re-read among them \
             keeps its right to conclude a loss"
        );
        assert!(!epoch.explicit_outstanding());
        assert_eq!(epoch.finish_explicit(reload), None, "and only once");
    }

    /// A mutation still supersedes an explicit reload (a save landing while its read is
    /// out), and then nothing has to wait for it.
    #[test]
    fn a_mutation_supersedes_an_explicit_reload_and_frees_the_re_reads() {
        let epoch = DocEpoch::default();
        let reload = epoch.claim_explicit();
        epoch.bump();
        assert!(!epoch.is_current(reload), "its answer predates the save");
        assert!(
            !epoch.explicit_outstanding(),
            "a doomed reload holds no background re-read back"
        );
    }

    /// A second Reload pressed while the first is out takes over: the first one's
    /// return leaves the re-reads that stood aside for the second to run.
    #[test]
    fn a_newer_explicit_reload_keeps_the_yielded_re_read() {
        let epoch = DocEpoch::default();
        let first = epoch.claim_explicit();
        let second = epoch.claim_explicit();
        epoch.yield_to_explicit(false);
        assert_eq!(epoch.finish_explicit(first), None);
        assert!(epoch.explicit_outstanding(), "the second is still out");
        assert_eq!(epoch.finish_explicit(second), Some(false));
    }

    #[test]
    fn one_documents_activity_does_not_supersede_anothers() {
        let a = DocEpoch::default();
        let b = DocEpoch::default();
        let ticket_a = a.claim();
        b.claim();
        b.bump();
        assert!(a.is_current(ticket_a));
    }

    /// The counter wraps rather than overflowing. A `u64` of document operations is
    /// unreachable, but a panic in the save path would be a poor way to find that out.
    #[test]
    fn the_counter_wraps_instead_of_overflowing() {
        let epoch = DocEpoch {
            current: Cell::new(u64::MAX),
            ..DocEpoch::default()
        };
        let ticket = epoch.claim();
        assert!(epoch.is_current(ticket));
        epoch.bump();
        assert!(!epoch.is_current(ticket));
    }

    /// **The save guard's own write is what moves under it.**
    ///
    /// The semantics behind `window/save.rs`'s re-read, proved as data: a mark taken
    /// before a guard read stops being current the moment one of our own saves lands,
    /// which is exactly the case where the bytes that came back are older than the
    /// baseline they are about to be compared against. Believe them and the user is
    /// asked to overwrite their own save.
    #[test]
    fn our_own_completed_save_invalidates_a_mark_taken_before_it() {
        let writes = WriteEpoch::default();
        let before = writes.observe(); // the guard read goes out
        assert!(writes.is_current(before));
        writes.bump(); // …and a save of ours lands while it is out
        assert!(
            !writes.is_current(before),
            "the guard must be able to tell that the disk it read predates the              baseline it is comparing against"
        );
    }

    /// Observing does not supersede — unlike [`DocEpoch::claim`], and deliberately.
    ///
    /// The save guard is not a reader that may win; it is a reader that must notice
    /// it has been overtaken. If observing bumped, two guards in flight would each
    /// invalidate the other and both would re-read forever.
    #[test]
    fn observing_the_write_epoch_supersedes_nobody() {
        let writes = WriteEpoch::default();
        let first = writes.observe();
        let second = writes.observe();
        assert!(writes.is_current(first));
        assert!(writes.is_current(second));
    }

    /// The write counter wraps too, for [`DocEpoch`]'s reason.
    #[test]
    fn the_write_counter_wraps_instead_of_overflowing() {
        let writes = WriteEpoch {
            current: Cell::new(u64::MAX),
        };
        let mark = writes.observe();
        writes.bump();
        assert!(!writes.is_current(mark));
    }
}
