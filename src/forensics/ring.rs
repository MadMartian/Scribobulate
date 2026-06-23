//! The breadcrumb ring — the last N things the application did, readable from a
//! signal handler.
//!
//! **This is the deliverable of the whole crash-forensics effort.** Every recovered
//! crash so far was dangling-GObject-shaped, and that class is diagnosed by knowing
//! *what the app was doing*, not by a backtrace: the faulting frame is inside GTK,
//! which carries no symbols on the reference platform (ScrAP-141). A ring of recent
//! lifecycle events is the only artefact that answers the question.
//!
//! # Why this is not a `Mutex<VecDeque<String>>`
//!
//! Because the reader is a SIGSEGV handler. Two things are unavailable there:
//!
//! * **Locks.** The fault can land *inside* a `log` call that already holds the
//!   mutex; re-acquiring it deadlocks, and the process hangs rather than dying —
//!   strictly worse than the silence this feature exists to fix.
//! * **Allocation.** `malloc` is not async-signal-safe, for the same
//!   already-holding-its-own-lock reason.
//!
//! So the storage is a fixed array of fixed-size slots, written with atomics only,
//! and the reader takes no lock and allocates nothing. Recording is wait-free: one
//! `fetch_add` picks a slot, a per-slot sequence number claims it, and the reader
//! skips any slot that is mid-write instead of blocking on it.
//!
//! # The slot protocol (a seqlock), and the two races it did not used to cover
//!
//! Each slot carries a `seq`: **0** means never written, **odd** means a writer holds
//! it, **even and non-zero** means it holds a complete record. A writer claims a slot by
//! `compare_exchange`ing an even `seq` to `seq + 1` and publishes by storing `seq + 2`;
//! a reader samples `seq`, copies the bytes, and re-samples — using the record only if
//! `seq` is unchanged and even throughout.
//!
//! Both halves of that were missing, and neither was visible in the safety argument:
//!
//! * **Writer versus writer** (QA round 5, M-3). The previous protocol *announced* a
//!   write (`state.store(WRITING)`) but never *claimed* the slot, so two threads whose
//!   indices are `CAPACITY` apart took `&mut` to the same buffer at the same time —
//!   aliasing UB, not merely a garbled line. The `unsafe impl Sync` justification below
//!   enumerated reader/writer and stopped there, and the multi-thread test could not see
//!   it: every record shared a layout, so a byte-level interleave still satisfied
//!   `starts_with("thread ")`. MEASURED with ThreadSanitizer (procedure and result under
//!   [`Ring::record`]); a `compare_exchange` claim is what removes it.
//! * **Reader versus writer** (F-SEC5-009). The reader checked the flag *before* reading
//!   and never again, which is a seqlock missing its second read: a writer that started
//!   after the check could rewrite the slot underneath it and the reader would emit the
//!   mixture. Re-sampling `seq` afterwards turns that from a garbled line into a skipped
//!   one.
//!
//! # The one accepted race
//!
//! A slot can be reused while the reader is walking it if `CAPACITY` records are
//! written between the reader starting and reaching that slot. The application is
//! single-threaded (TECH.md § Concurrency model) and the reader runs *in* the
//! faulting thread with everything else stopped, so that cannot happen in practice;
//! were it to, the re-sample above now discards the record rather than printing a
//! mixture of two.
//!
//! What remains formally is that the byte copy either side of a `seq` change is a
//! non-atomic access the abstract machine calls a race even though the result is
//! thrown away — the standard, unavoidable seqlock caveat in today's Rust, there being
//! no per-byte atomic load. It is mitigated the standard way (`read_volatile`, so the
//! compiler cannot exploit it) and it is unreachable under the premise above, which is
//! why it is documented rather than designed around. This is the honest boundary of the
//! guarantee: writer/writer is *fixed*, reader/writer is *bounded*.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

/// How many breadcrumbs are retained. Sized from the plan's "last ~64 things that
/// happened": enough to span a user gesture and the machinery it sets off, small
/// enough that the whole ring is one glance in a report.
pub(crate) const CAPACITY: usize = 64;

/// Bytes per breadcrumb. A lifecycle record is `timestamp LEVEL target: message`;
/// 224 holds that plus a long absolute path. Longer records truncate (see
/// [`Ring::record`]) rather than allocating.
pub(crate) const ENTRY_BYTES: usize = 224;

/// A slot never written. Distinguished from a written one so a partially-filled ring
/// reports only what it holds.
const UNWRITTEN: usize = 0;

struct Slot {
    /// 0 = never written, odd = claimed by a writer, even and non-zero = complete.
    seq: AtomicUsize,
    len: AtomicUsize,
    text: UnsafeCell<[u8; ENTRY_BYTES]>,
}

impl Slot {
    const fn new() -> Self {
        Self {
            seq: AtomicUsize::new(UNWRITTEN),
            len: AtomicUsize::new(0),
            text: UnsafeCell::new([0; ENTRY_BYTES]),
        }
    }
}

// SAFETY: the `UnsafeCell` is written only by the thread that won this slot's
// `compare_exchange` claim (`seq` even → odd), and no other writer can take a `&mut`
// to it until that thread publishes `seq + 2`. That is the writer/writer half, and it
// is the half the previous justification omitted — announcing a write is not claiming
// one (M-3; ThreadSanitizer output quoted on `Ring::record`).
//
// A reader never takes a reference into the cell at all: it copies the bytes out and
// re-checks `seq`, discarding the copy unless the slot was complete and unchanged
// across the read. The orderings pair — Acquire on the claim and on both reader
// samples, Release on the publish — so a reader that sees an unchanged even `seq` also
// sees the bytes and the length that preceded it.
unsafe impl Sync for Slot {}

/// A fixed-capacity ring of recent log records.
///
/// The process holds one of these in a `static` ([`super::breadcrumbs`]); tests
/// construct their own on the stack, which is why this is a type rather than a set
/// of free functions over module-level statics — a global would make every test of
/// it order-dependent under `cargo test`'s thread pool.
pub(crate) struct Ring {
    slots: [Slot; CAPACITY],
    next: AtomicUsize,
}

impl Ring {
    pub(crate) const fn new() -> Self {
        Self {
            // Inline `const` rather than a named one: a named constant holding
            // atomics trips `clippy::declare_interior_mutable_const`, and rightly —
            // each repeat must construct a *fresh* slot, which is what this spells.
            slots: [const { Slot::new() }; CAPACITY],
            next: AtomicUsize::new(0),
        }
    }

    /// Record one breadcrumb, truncating to [`ENTRY_BYTES`] on a character
    /// boundary. Never blocks, never allocates, never fails.
    ///
    /// # Why the claim is a `compare_exchange` and not a store
    ///
    /// `fetch_add` hands every writer a distinct *index*, but two indices `CAPACITY`
    /// apart share a *slot*. The previous version stored `WRITING` and copied, which
    /// told readers to stay away but did nothing about the other writer, so both took
    /// `&mut` to the same buffer. MEASURED under ThreadSanitizer — 4 threads × 20 000
    /// records through this exact code, `-Zsanitizer=thread`:
    ///
    /// ```text
    /// WARNING: ThreadSanitizer: data race
    ///   #0 __tsan_memcpy
    ///   #1 ringtsan::Ring::record          <- both stacks are `record`
    /// ```
    ///
    /// Claiming the slot makes the loser skip its breadcrumb instead. That is the right
    /// trade: the application is single-threaded, so under the documented premise no
    /// claim is ever contended and nothing is dropped; if the premise is ever broken, a
    /// missing diagnostic line is a cost and aliasing UB in the crash reporter is not.
    /// It also keeps the "never blocks" contract — the loser returns, it does not spin.
    pub(crate) fn record(&self, line: &str) {
        let index = self.next.fetch_add(1, Ordering::Relaxed);
        let slot = &self.slots[index % CAPACITY];

        let seq = slot.seq.load(Ordering::Relaxed);
        // Odd: another writer holds this slot. Even but lost: another writer claimed it
        // between the load and the exchange. Either way this record is dropped rather
        // than written into a buffer somebody else owns.
        if !seq.is_multiple_of(2)
            || slot
                .seq
                .compare_exchange(seq, seq + 1, Ordering::Acquire, Ordering::Relaxed)
                .is_err()
        {
            return;
        }

        let take = super::floor_char_boundary(line, ENTRY_BYTES);
        // SAFETY: this thread won the claim above, so it holds the slot exclusively
        // until it publishes `seq + 2` below — no other writer can take a reference to
        // the cell, and a reader never takes one at all. `take <= ENTRY_BYTES` bounds
        // the copy to the array.
        unsafe {
            let dst = &mut *slot.text.get();
            dst[..take].copy_from_slice(&line.as_bytes()[..take]);
        }
        slot.len.store(take, Ordering::Relaxed);
        slot.seq.store(seq + 2, Ordering::Release);
    }

    /// Visit every recorded breadcrumb, oldest first.
    ///
    /// **Async-signal-safe**: no locks, no allocation, no reentrant calls. `visit`
    /// must honour the same rule when this is called from a signal handler — the
    /// handler passes a closure that does nothing but `write(2)`.
    ///
    /// The copy is a `[u8; ENTRY_BYTES]` stack temporary, which is what lets the slot be
    /// re-validated *before* the bytes are handed to `visit` — a seqlock reader that
    /// hands out a borrow into the slot has already lost, because it cannot un-print a
    /// line it discovers was torn. 224 bytes against a 64 KiB alternate signal stack.
    pub(crate) fn for_each<F: FnMut(&[u8])>(&self, mut visit: F) {
        let next = self.next.load(Ordering::Acquire);
        let first = next.saturating_sub(CAPACITY);
        for index in first..next {
            let slot = &self.slots[index % CAPACITY];
            let before = slot.seq.load(Ordering::Acquire);
            if before == UNWRITTEN || !before.is_multiple_of(2) {
                // Either never written (a ring that has not filled yet) or being
                // written right now by the frame we interrupted. Skipping is the
                // point: waiting for it is the deadlock this design avoids.
                continue;
            }
            let len = slot.len.load(Ordering::Relaxed).min(ENTRY_BYTES);
            // SAFETY: an even, non-zero `seq` was observed with Acquire ordering, which
            // pairs with the Release publish in `record`, so the bytes and the length
            // that preceded it are visible. `read_volatile` rather than a reference: a
            // writer may still claim this slot mid-copy, and the copy is discarded below
            // if it did — volatile keeps the compiler from assuming that cannot happen
            // (see the module header on the residual seqlock caveat).
            let snapshot = unsafe { std::ptr::read_volatile(slot.text.get()) };
            if slot.seq.load(Ordering::Acquire) != before {
                // A writer took the slot while it was being copied: the bytes are a
                // mixture of two records, so drop them rather than report them. This
                // second sample is what makes the protocol a seqlock (F-SEC5-009).
                continue;
            }
            visit(&snapshot[..len]);
        }
    }

    /// Claim slot `index`, write `line` into it, and **stop before publishing** — the
    /// exact state a writer is in when a second writer arrives at the same slot.
    ///
    /// The contended cases are otherwise only reachable by racing two real threads,
    /// which is precisely the kind of guard that passes for the wrong reason: a stress
    /// test over records that all look alike cannot tell a tear from a clean write, and
    /// that is what let M-3 through. Suspending a writer mid-write turns the race into
    /// an ordinary assertion.
    ///
    /// It writes real content deliberately. A helper that only took the claim would let
    /// a test assert the *reader skips the slot* — which is true whether or not the
    /// second writer trampled the buffer, since the slot stays odd either way. The
    /// hazard is the trampling, so the held slot has to carry bytes worth checking.
    #[cfg(test)]
    fn begin_write_for_test(&self, index: usize, line: &str) {
        let slot = &self.slots[index % CAPACITY];
        let seq = slot.seq.load(Ordering::Relaxed);
        slot.seq
            .compare_exchange(seq, seq + 1, Ordering::Acquire, Ordering::Relaxed)
            .expect("uncontended in a single-threaded test");
        let take = super::floor_char_boundary(line, ENTRY_BYTES);
        // SAFETY: the claim above is held by this thread and not yet published.
        unsafe {
            let dst = &mut *slot.text.get();
            dst[..take].copy_from_slice(&line.as_bytes()[..take]);
        }
        slot.len.store(take, Ordering::Relaxed);
    }

    /// Let the suspended writer of [`begin_write_for_test`] finish and publish.
    #[cfg(test)]
    fn finish_write_for_test(&self, index: usize) {
        let slot = &self.slots[index % CAPACITY];
        let seq = slot.seq.load(Ordering::Relaxed);
        assert!(
            !seq.is_multiple_of(2),
            "no suspended writer holds slot {index}"
        );
        slot.seq.store(seq + 1, Ordering::Release);
    }

    /// Every breadcrumb as text, oldest first. Test-facing counterpart of
    /// [`Ring::for_each`] — the report writers use `for_each` directly so nothing
    /// on the crash path allocates.
    #[cfg(test)]
    pub(crate) fn snapshot(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.for_each(|bytes| out.push(String::from_utf8_lossy(bytes).into_owned()));
        out
    }
}

/// The largest prefix length of `line` that is `<= limit` and a character boundary.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_in_order() {
        let ring = Ring::new();
        ring.record("opened /tmp/a.md");
        ring.record("saved /tmp/a.md");
        assert_eq!(ring.snapshot(), vec!["opened /tmp/a.md", "saved /tmp/a.md"]);
    }

    #[test]
    fn an_untouched_ring_yields_nothing() {
        assert!(Ring::new().snapshot().is_empty());
    }

    #[test]
    fn keeps_the_newest_capacity_records_and_drops_the_oldest() {
        // The property the crash report depends on: what survives is the recent
        // past, never the distant one (TDD 21.2's principle, applied in memory).
        let ring = Ring::new();
        for n in 0..CAPACITY + 10 {
            ring.record(&format!("event {n}"));
        }
        let seen = ring.snapshot();
        assert_eq!(seen.len(), CAPACITY);
        assert_eq!(seen.first().map(String::as_str), Some("event 10"));
        assert_eq!(
            seen.last().map(String::as_str),
            Some(format!("event {}", CAPACITY + 9).as_str())
        );
    }

    #[test]
    fn a_partially_filled_ring_reports_only_what_was_written() {
        let ring = Ring::new();
        for n in 0..3 {
            ring.record(&format!("event {n}"));
        }
        assert_eq!(ring.snapshot().len(), 3);
    }

    #[test]
    fn an_over_long_record_truncates_rather_than_growing() {
        let ring = Ring::new();
        ring.record(&"x".repeat(ENTRY_BYTES * 2));
        let seen = ring.snapshot();
        assert_eq!(seen[0].len(), ENTRY_BYTES);
    }

    #[test]
    fn truncation_lands_on_a_character_boundary() {
        // `from_utf8_lossy` would hide a split character behind U+FFFD, so assert
        // the bytes parse strictly.
        let ring = Ring::new();
        ring.record(&"é".repeat(ENTRY_BYTES)); // 2 bytes each
        ring.for_each(|bytes| {
            assert!(std::str::from_utf8(bytes).is_ok());
            assert_eq!(bytes.len(), ENTRY_BYTES);
        });
    }

    /// A slot a writer is inside is invisible to the reader (M-3 / F-SEC5-009).
    #[test]
    fn a_slot_held_by_a_writer_is_skipped_by_the_reader() {
        let ring = Ring::new();
        ring.record("complete");
        ring.record("about to be overwritten");
        ring.begin_write_for_test(1, "half written"); // a writer is inside slot 1

        assert_eq!(
            ring.snapshot(),
            vec!["complete"],
            "a slot being written must be skipped, never reported half-written"
        );
    }

    /// Two writers cannot hold one slot — the aliasing M-3 was.
    ///
    /// `fetch_add` gives every writer its own index, but indices `CAPACITY` apart share
    /// a slot, and the old protocol only *announced* the write. MEASURED under
    /// ThreadSanitizer on the pre-fix code: `data race … #0 __tsan_memcpy #1
    /// Ring::record` with `record` on both stacks; clean after the claim was added
    /// (4 threads × 20 000 records, plus a concurrent reader). That run is the real
    /// positive control and is reproducible; this test is its deterministic residue, so
    /// the invariant is guarded by `cargo test` on every change rather than by
    /// remembering to run a sanitizer.
    ///
    /// **The assertion is on the held writer's BYTES, not on the reader's skip**, and
    /// the difference is the whole test. A slot held by a writer stays odd whatever a
    /// second writer does to its buffer, so `snapshot()` returns the same thing either
    /// way — a first version of this test asserted exactly that and survived the
    /// mutation it was written to kill. Letting the held writer finish and then reading
    /// what is in the slot is what distinguishes "the intruder backed off" from "the
    /// intruder wrote into a buffer another writer owned".
    ///
    /// Mutation guard: drop the claim so `record` writes unconditionally, and the slot
    /// comes back holding the intruder's record instead of the holder's.
    #[test]
    fn a_second_writer_cannot_take_a_slot_another_writer_holds() {
        let ring = Ring::new();
        // A writer is inside slot 0 with its bytes already copied, not yet published.
        ring.begin_write_for_test(0, "the record whose buffer this is");

        // A second writer arrives at the same slot (index 0 → slot 0).
        ring.record("the intruder");

        // Let the original writer finish, and see whose bytes are there.
        ring.finish_write_for_test(0);
        assert_eq!(
            ring.snapshot(),
            vec!["the record whose buffer this is"],
            "a writer that loses the claim must drop its record — writing into a slot \
             another writer is inside is aliasing UB, not a garbled line"
        );

        // …and the drop is the claim, not a ring that refuses everything: an unheld
        // slot still takes its record.
        ring.record("kept");
        assert_eq!(
            ring.snapshot(),
            vec!["the record whose buffer this is", "kept"]
        );
    }

    #[test]
    fn recording_from_several_threads_never_tears_a_record() {
        // The application is single-threaded, but `log` is a process-global facade
        // any thread may reach; a torn breadcrumb would be unsound, not just ugly.
        //
        // Each thread writes one repeated character filling the whole slot, so ANY
        // byte-level interleaving shows up as a line with two distinct bytes in it. The
        // previous shape — `thread {t} event {n:03}` — could not discriminate: every
        // record had the same layout and nearly the same bytes, so a mixture still
        // satisfied `starts_with("thread ")`. It is honest about its limit even so: a
        // stress test detects tearing only if the interleave happens to land in this
        // run, which is why the guarantee rests on the two deterministic tests above
        // and the sanitizer run, and this one is a backstop.
        let ring = std::sync::Arc::new(Ring::new());
        let threads: Vec<_> = (0..8)
            .map(|t| {
                let ring = ring.clone();
                std::thread::spawn(move || {
                    let line: String =
                        std::iter::repeat_n(char::from(b'a' + t), ENTRY_BYTES).collect();
                    for _ in 0..2_000 {
                        ring.record(&line);
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        for line in ring.snapshot() {
            let first = line.as_bytes()[0];
            assert!(
                line.as_bytes().iter().all(|b| *b == first),
                "torn record — one slot holds bytes from two writers: {line:?}"
            );
        }
    }
}
