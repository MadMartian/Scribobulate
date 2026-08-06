//! Dispatching blocking work to GLib's I/O thread pool — and the bound on how
//! much of that pool this application is allowed to occupy.
//!
//! # The pool is shared, and the crash-recovery snapshot writer is in it
//!
//! `gio::spawn_blocking` and `GFile`'s own `*_async` operations are the **same
//! process-wide pool**: `GLocalFile` overrides no async `GFile` vfuncs, so
//! `g_file_replace_async` falls through to `g_file_real_replace_async` →
//! `g_task_run_in_thread`, and `GLocalFileOutputStream` implements no pollable
//! interface, so `g_output_stream_async_write_is_via_threads()` is TRUE and
//! `goutputstream.c:1225` dispatches there too. The pool is a single file-static
//! (`gtask.c:619`) created with **10 threads**:
//!
//! ```c
//! /* gtask.c:643  */ #define G_TASK_POOL_SIZE 10
//! /* gtask.c:2195 */ task_pool = g_thread_pool_new (g_task_thread_pool_thread, NULL,
//!                                                   G_TASK_POOL_SIZE, FALSE, NULL);
//! ```
//!
//! So every document read and write added by this module contends with the
//! crash-recovery snapshot writer (`window/swap.rs`), which is the mechanism that
//! protects the user's unsaved work.
//!
//! # It is not starvation — it is latency, which here is the same thing
//!
//! The pool does grow: `gtask.c:629-646` adds **one** thread when tasks have been
//! blocking, on a compounding wait (100 ms base, ×1.03 per running task), and that
//! manager runs on GLib's own worker thread so it survives a wedged main loop. A
//! queued snapshot therefore always completes eventually.
//!
//! "Eventually" is the problem. Measured (researcher, GLib 2.72: N tasks blocked
//! forever on an empty pipe, then a 64 KiB snapshot write, timed to completion):
//!
//! | tasks blocked | snapshot completes after |
//! |---|---|
//! | 9  | **0.2 ms** |
//! | 10 | **206.6 ms** |
//! | 15 | 686 ms |
//! | 20 | 1.36 s |
//! | 50 | 8.37 s |
//!
//! **The cliff is exactly the base pool size.** Nine blocked tasks cost nothing;
//! the tenth costs 200 ms, and it climbs from there. A crash-recovery snapshot that
//! is late is unsaved work that is unprotected for exactly that long — which is the
//! window the whole feature exists to close.
//!
//! Nothing else is a lever. There is no public API to give the snapshot writer its
//! own pool, and `io_priority` does not help: `gtask.c:2199` sorts the queue by
//! `blocking_other_task` first, and that flag is set only for tasks queued from
//! *inside* a pool thread (`:1534`), which none of ours are.
//!
//! # Hence [`MAX_CONCURRENT`]
//!
//! This module admits at most four document operations to the pool at once, so at
//! least six of the base ten threads are always free and a snapshot never crosses
//! the cliff — however many tabs change at the same instant, and however slow the
//! filesystem holding them is. Anything over the limit waits **here**, in the
//! application, where waiting costs nothing.
//!
//! The fan-out is real rather than theoretical: session restore and the multi-file
//! open both read strictly sequentially, but the live-reload monitor fires one read
//! per affected tab, and one `git checkout` across a documentation tree rewrites
//! every open document at once.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

/// How many document operations may occupy GLib's I/O thread pool at once.
///
/// Four, against a base pool of ten, leaves six threads free — comfortably clear of
/// the measured cliff at the tenth blocked task (see this module's doc comment).
/// It is a bound on *held threads*, not on throughput: a local-disk read returns in
/// microseconds and never queues here at all.
const MAX_CONCURRENT: usize = 4;

thread_local! {
    /// Main-thread-only, so plain interior mutability with no locking: every future
    /// that touches this runs on the GTK main thread via `spawn_local`, which is the
    /// same property the rest of the application already relies on.
    static GATE: RefCell<Gate> = const {
        RefCell::new(Gate { running: 0, waiting: VecDeque::new() })
    };
}

struct Gate {
    running: usize,
    waiting: VecDeque<Waiter>,
}

struct Waiter {
    /// Shared with the waiting [`Acquire`]; set when a released slot is handed to
    /// it, which is what stops a woken waiter from re-queueing behind newcomers.
    granted: Rc<Cell<bool>>,
    waker: Waker,
}

use std::cell::Cell;

/// One admitted operation. Releasing it — on drop, so an early return or a panic
/// cannot leak it — hands the slot to the longest-waiting caller, or gives it back
/// to the pool if nobody is waiting.
struct Slot;

impl Drop for Slot {
    fn drop(&mut self) {
        // The waker is woken OUTSIDE the borrow: waking can poll a future
        // synchronously, and that future's first act is to look at this same
        // `RefCell`.
        let next = GATE.with(|gate| {
            let mut gate = gate.borrow_mut();
            match gate.waiting.pop_front() {
                // Hand the slot straight over: `running` is unchanged because the
                // count did not drop, it moved.
                Some(waiter) => {
                    waiter.granted.set(true);
                    Some(waiter.waker)
                }
                None => {
                    gate.running -= 1;
                    None
                }
            }
        });
        if let Some(waker) = next {
            waker.wake();
        }
    }
}

/// Wait for a free slot. See [`Slot`].
struct Acquire {
    granted: Rc<Cell<bool>>,
    queued: bool,
}

impl Future for Acquire {
    type Output = Slot;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Slot> {
        let this = self.get_mut();
        if this.granted.get() {
            // A slot was handed to us by a departing holder; the count already
            // reflects it and we are no longer in the queue. The flag is CONSUMED
            // here — leaving it set would make this future's own drop release a
            // slot the returned `Slot` is still holding, i.e. hand the same one out
            // twice and underflow the count when both are eventually returned.
            this.granted.set(false);
            this.queued = false;
            return Poll::Ready(Slot);
        }
        GATE.with(|gate| {
            let mut gate = gate.borrow_mut();
            if !this.queued && gate.running < MAX_CONCURRENT {
                gate.running += 1;
                return Poll::Ready(Slot);
            }
            // Re-registering replaces our waker rather than adding a second one: a
            // future may legitimately be polled with a different waker than last
            // time, and only the newest one is guaranteed to reach the executor.
            gate.waiting
                .retain(|w| !Rc::ptr_eq(&w.granted, &this.granted));
            gate.waiting.push_back(Waiter {
                granted: Rc::clone(&this.granted),
                waker: cx.waker().clone(),
            });
            this.queued = true;
            Poll::Pending
        })
    }
}

impl Drop for Acquire {
    fn drop(&mut self) {
        if !this_was_queued(self) {
            return;
        }
        // Dropped while waiting — deregister, and if a slot had already been handed
        // over in the same turn, release it rather than losing it forever. Nothing
        // in this crate cancels these futures today; the guard is here because a
        // leaked slot is permanent and silent, and would shrink the limit for the
        // rest of the session.
        let granted = self.granted.get();
        GATE.with(|gate| {
            let mut gate = gate.borrow_mut();
            gate.waiting
                .retain(|w| !Rc::ptr_eq(&w.granted, &self.granted));
            if granted {
                gate.running -= 1;
            }
        });
    }
}

/// Whether `a` ever entered the queue (or was handed a slot), i.e. whether its drop
/// has any bookkeeping to undo.
fn this_was_queued(a: &Acquire) -> bool {
    a.queued || a.granted.get()
}

/// Run `f` on GLib's I/O thread pool and resume on the main context with its
/// result, waiting first for a free slot if this application already has
/// [`MAX_CONCURRENT`] operations out.
///
/// Only plain owned data crosses the boundary in either direction; **no GTK object
/// may be captured by `f`**, which is why every caller in this module takes owned
/// `PathBuf`/`String` rather than borrowing from a widget or a `TabState`.
///
/// `f` can never run on the calling thread: `g_task_start_task_thread` has exactly
/// two exits and both are `g_thread_pool_push` (`gtask.c:1516`, `:1536`) — there is
/// no inline branch and no pool-creation fallback (`g_assert` at `:2197`).
///
/// A panic inside `f` is caught by the pool and re-raised **here**, on the main
/// thread. That is not incidental: `main.rs` installs a panic hook that writes a
/// crash report, and POLICY § Logging forbids `panic = "abort"` precisely so it
/// runs. Swallowing the panic into an error would silently downgrade a programmer
/// error into a failed save.
pub(super) async fn off_main<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let _slot = Acquire {
        granted: Rc::new(Cell::new(false)),
        queued: false,
    }
    .await;
    #[cfg(test)]
    let f = {
        let delay = injected_delay();
        move || {
            // On the POOL thread, exactly where a slow filesystem's latency lands, so
            // the main loop keeps running throughout — a `sleep` here reproduces the
            // condition rather than merely postponing the test.
            std::thread::sleep(delay);
            f()
        }
    };
    match gtk::gio::spawn_blocking(f).await {
        Ok(value) => value,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

// Test-only: how long each dispatched operation should pretend the filesystem took.
//
// The behaviours that only exist while I/O is slow — a second Save being dropped
// rather than raced, a reload's completion arriving after a save's, a window staying
// responsive throughout — are unreachable on a local disk, where every operation
// returns before anything can be observed. `scripts/slowfs.py` mounts a real FUSE
// filesystem that answers slowly and is the right tool for a *manual* pass; this is
// its in-process equivalent, so the same conditions can be pinned by an ordinary test
// on every platform, with no mount, no root and no external tooling.
//
// `#[cfg(test)]` throughout, deliberately: an env-var-gated version was considered and
// rejected because it would put a fault-injection switch in the shipped binary. Here
// the delay does not exist in a release build at all.
#[cfg(test)]
thread_local! {
    static INJECTED_DELAY: Cell<std::time::Duration> = const {
        Cell::new(std::time::Duration::ZERO)
    };
}

#[cfg(test)]
fn injected_delay() -> std::time::Duration {
    INJECTED_DELAY.with(|d| d.get())
}

/// Make every document operation take at least `delay`, until the returned guard is
/// dropped. Restores the previous value rather than zeroing, so nesting is safe and a
/// panicking test cannot leave the delay set for whatever runs next on this thread.
// Gated to its callers' cfg (the gtk-integration-tests modules), not the broader
// `cfg(test)` — otherwise a bare `cargo test` compiles the injector with nothing to
// inject into and reports it, its guard, and the re-export as dead.
#[cfg(all(test, feature = "gtk-integration-tests"))]
#[must_use = "the delay is only in force while the guard is alive"]
pub(crate) fn slow_io(delay: std::time::Duration) -> SlowIoGuard {
    let previous = INJECTED_DELAY.with(|d| d.replace(delay));
    SlowIoGuard { previous }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) struct SlowIoGuard {
    previous: std::time::Duration,
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
impl Drop for SlowIoGuard {
    fn drop(&mut self) {
        INJECTED_DELAY.with(|d| d.set(self.previous));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How many slots are currently held. Test-only view of the gate.
    fn running() -> usize {
        GATE.with(|g| g.borrow().running)
    }

    fn waiting() -> usize {
        GATE.with(|g| g.borrow().waiting.len())
    }

    /// Take a slot that is expected to be free. Deliberately does NOT involve a
    /// main context: `Acquire` is a plain future over a thread-local counter, so
    /// these tests need no GLib main loop, no display, and no thread affinity —
    /// which is what keeps the module's whole decision surface inside the headless
    /// coverage gate.
    fn claim_now() -> Slot {
        let mut fut = Box::pin(claim());
        match poll_once(&mut fut) {
            Poll::Ready(slot) => slot,
            Poll::Pending => panic!("a claim under the limit must be admitted at once"),
        }
    }

    /// The bound is the whole point of this module, so it is asserted directly
    /// rather than inferred from timing: with `MAX_CONCURRENT` operations out, the
    /// next one does not reach the pool.
    ///
    /// Mutation: raising `MAX_CONCURRENT` past the pool's base size, or dropping the
    /// `Acquire` await from `off_main`, fails this.
    #[test]
    fn no_more_than_the_limit_reach_the_pool_at_once() {
        let held: Vec<Slot> = (0..MAX_CONCURRENT).map(|_| claim_now()).collect();
        assert_eq!(running(), MAX_CONCURRENT);

        // A further claim cannot complete while the limit is held — poll it once and
        // confirm it parked rather than being admitted.
        let mut extra = Box::pin(claim());
        assert!(poll_once(&mut extra).is_pending());
        assert_eq!(waiting(), 1, "the over-limit claim is queued, not admitted");
        assert_eq!(running(), MAX_CONCURRENT, "and did not raise the count");

        drop(extra);
        drop(held);
        assert_eq!(running(), 0, "every slot is returned");
        assert_eq!(waiting(), 0, "and no waiter is left behind");
    }

    /// A released slot is handed to the waiter that has been queued longest, and the
    /// count never dips in between — the transfer is what stops a woken waiter from
    /// losing its place to a newcomer.
    #[test]
    fn a_released_slot_is_handed_to_the_longest_waiter() {
        let held: Vec<Slot> = (0..MAX_CONCURRENT).map(|_| claim_now()).collect();
        let mut first = Box::pin(claim());
        let mut second = Box::pin(claim());
        assert!(poll_once(&mut first).is_pending());
        assert!(poll_once(&mut second).is_pending());
        assert_eq!(waiting(), 2);

        let mut held = held;
        drop(held.pop());
        assert_eq!(
            running(),
            MAX_CONCURRENT,
            "the count does not dip: the slot moved rather than being returned"
        );
        // Bound rather than discarded: dropping the returned `Slot` inline would
        // immediately hand it on to `second` and the next assertion would be
        // testing the opposite of what it says.
        let taken = match poll_once(&mut first) {
            Poll::Ready(slot) => slot,
            Poll::Pending => panic!("the longest-waiting claim must take the freed slot"),
        };
        assert!(
            poll_once(&mut second).is_pending(),
            "the newer one still waits"
        );

        drop(taken);
        drop(second);
        drop(held);
        assert_eq!(running(), 0);
        assert_eq!(waiting(), 0);
    }

    /// A claim abandoned while queued leaves nothing behind. A leaked slot would be
    /// permanent and silent — the limit would simply be smaller for the rest of the
    /// session, with no symptom until a snapshot went late.
    #[test]
    fn abandoning_a_queued_claim_leaks_no_slot() {
        let held: Vec<Slot> = (0..MAX_CONCURRENT).map(|_| claim_now()).collect();
        {
            let mut abandoned = Box::pin(claim());
            assert!(poll_once(&mut abandoned).is_pending());
            assert_eq!(waiting(), 1);
        }
        assert_eq!(waiting(), 0, "the abandoned claim deregistered itself");
        drop(held);
        assert_eq!(running(), 0);
    }

    /// …including one abandoned in the window between being HANDED a slot and being
    /// polled to notice, which is the case that actually loses one.
    #[test]
    fn abandoning_a_claim_that_was_already_handed_a_slot_leaks_no_slot() {
        let mut held: Vec<Slot> = (0..MAX_CONCURRENT).map(|_| claim_now()).collect();
        let mut pending = Box::pin(claim());
        assert!(poll_once(&mut pending).is_pending());
        drop(held.pop()); // hands the slot over, but `pending` is never polled again
        assert_eq!(running(), MAX_CONCURRENT);
        drop(pending);
        assert_eq!(running(), MAX_CONCURRENT - 1, "the handed slot came back");
        drop(held);
        assert_eq!(running(), 0);
    }

    fn claim() -> impl Future<Output = Slot> {
        Acquire {
            granted: Rc::new(Cell::new(false)),
            queued: false,
        }
    }

    /// Poll a future exactly once with a no-op waker. Enough to observe whether it
    /// was admitted, without an executor that would drive it to completion.
    fn poll_once<F: Future>(fut: &mut Pin<Box<F>>) -> Poll<F::Output> {
        let waker = noop_waker();
        fut.as_mut().poll(&mut Context::from_waker(&waker))
    }

    fn noop_waker() -> Waker {
        use std::task::{RawWaker, RawWakerVTable};
        const VTABLE: RawWakerVTable = RawWakerVTable::new(
            |_| RawWaker::new(std::ptr::null(), &VTABLE),
            |_| {},
            |_| {},
            |_| {},
        );
        // SAFETY: every vtable entry is a no-op over a null data pointer, so the
        // waker never dereferences it.
        unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
    }
}
