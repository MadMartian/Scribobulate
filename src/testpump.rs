//! The one shared main-loop pump for every `gtk-integration-tests` body (M31).
//!
//! # Why this module exists
//!
//! Before it, `grep -rn "    fn pump" src/ --include=*.rs` found ~24 hand-rolled
//! copies of "iterate the main loop until a condition holds" across 19 files, in at
//! least eight distinct signatures. `src/clipboard.rs` alone added a third shape in
//! one merge — `present()`'s inline pump, a bare `pump(ctx, budget)`, and a THIRD
//! inline loop hand-rolled inside a single test — because the module-level `pump` it
//! already had took no predicate.
//!
//! Copy-count is the cheap half of that cost. **The expensive half is that the crate
//! had learned two things about pumping a GTK main loop that the copies did not
//! share**, and a fresh copy has no way to inherit a lesson recorded in a sibling
//! file's doc comment:
//!
//! * **GTK4Rs/AP-261 ("turns are not time"):** pumping the main context N times is
//!   not the same as letting N milliseconds pass, and different work wants opposite
//!   things. Idle-driven work (`GtkTextView` line-height validation, a signal fired
//!   from a dispatched idle) wants a TIGHT pump — a sleep between turns only delays a
//!   source that is already runnable, and measurably so (a 2 ms inter-turn sleep
//!   stretched a 100 ms settle past 3 s). Frame-clock-driven work (a scroll
//!   animation, `add_tick_callback`, a CSS transition) needs WALL-CLOCK time to pass,
//!   because the frame clock advances off its own periodic redraw source and a spin
//!   that never blocks never lets that source run. Work dispatched from GLib's own
//!   private worker thread (a `gio::spawn_blocking`/thread-pool completion, a freshly
//!   attached `GFileMonitor` — GTK4Rs/AP-269) needs wall-clock time too: no number of
//!   turns substitutes for giving another thread a chance to run and post back.
//! * **GTK4Rs/AP-79:** bound a blocking pump with a timeout SOURCE installed before
//!   the loop starts, never a wall-clock check made only *between* iterations — the
//!   latter hangs forever on an idle display, because a blocking `iteration(true)`
//!   with nothing pending simply never returns for the check to run.
//!
//! `docio::settle` (`pub(crate)` under this same feature gate, and therefore already
//! reachable from every one of these call sites) is the one pre-existing helper that
//! carried both lessons correctly: a real timeout **source**, and a **blocking**
//! `iteration(true)` loop rather than a spin. This module generalises that shape
//! rather than re-deriving it, and is where `docio::settle`'s own logic now lives —
//! see [`until_or_for`].
//!
//! # The API makes the clock an explicit choice
//!
//! The mechanism below — a watchdog **source** installed before a **blocking**
//! `iteration(true)` loop — is already correct for all three clocks above: it waits
//! exactly until the next dispatchable source, whichever clock schedules it, never
//! less and never an arbitrary sleep more. So [`Clock`] does not select a different
//! algorithm. It exists because the copies this module replaces each independently
//! guessed at a shape — some avoided blocking loops entirely (a fixed turn budget
//! with a manual sleep between iterations, which is wrong for idle-driven work per
//! GTK4Rs/AP-261's own measurement), some got the blocking shape right but only for the one
//! clock they had in front of them — and nothing recorded *why* one call site's
//! choice would be wrong for another's condition. Naming the clock at every call
//! site is what stops the next author from copying the wrong shape by analogy: there
//! is no default, so a call that doesn't know what it's waiting on has to find out.
//!
//! # What this module deliberately does not absorb
//!
//! Not every pump helper in the crate fits the "run until a predicate holds" shape
//! this module expresses, and forcing one into it would change what a timing-sensitive
//! test actually waits on — worse than leaving the duplication. Left in place, each
//! with a comment naming why:
//! * `window/rename.rs::pump_for` — a fixed wall-clock SPAN with no predicate at all
//!   ("pump for 1s while the inotify worker lands its event"), not an "until" wait.
//! * `preview/render.rs::pump_until` — called BOTH as a predicate wait and as an
//!   unconditional fixed-turn drain (`pump_until(&ctx, 200, || false)`); the second
//!   use would become an unconditional block-to-deadline here, changing the test's
//!   timing rather than just its spelling.
//! * `widgets/tab/bar.rs::pump_strip` and `preview/annotate/overlay.rs::pump_find_entry`
//!   — not predicate pumps at all (a tick-counting animation-settle heuristic and a
//!   widget finder, respectively); excluded from this module's count on the same
//!   grounds the M31 finding excluded them.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;

/// Which clock the awaited condition actually advances on (GTK4Rs/AP-261). Purely
/// documentation plus a per-clock default deadline — see the module rustdoc for why
/// the pump mechanism itself does not branch on this — but every call site names one,
/// because "however many turns looked enough" is the shape every duplicate this
/// module replaces shared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Clock {
    /// A main-context idle/timeout source: `GtkTextView` layout/text validation, a
    /// signal fired from a dispatched idle, a `gio` async completion posted back to
    /// this context.
    Idle,
    /// A real timer a human would perceive: the GTK frame clock (a scroll animation,
    /// `add_tick_callback`, a CSS transition) or a plain `glib::timeout_add`-driven
    /// wait (a debounce, a timed notice). Both need actual wall-clock time to pass,
    /// which is what distinguishes this from [`Clock::Idle`].
    Frame,
    /// Work GLib dispatches from its own private worker thread: a
    /// `gio::spawn_blocking`/thread-pool I/O completion, a freshly attached
    /// `GFileMonitor` (GTK4Rs/AP-269). No number of turns substitutes for giving the
    /// other thread a chance to run and post back.
    Worker,
}

impl Clock {
    /// A generous per-clock ceiling. A FAILURE bound (GTK4Rs/AP-122), never the
    /// completion signal itself — `done()` observing real state is always what ends
    /// the wait; use the `_for` variants to override it at a call site with a
    /// sharper bound of its own.
    fn default_deadline(self) -> Duration {
        match self {
            Clock::Idle => Duration::from_secs(20),
            Clock::Frame => Duration::from_secs(30),
            Clock::Worker => Duration::from_secs(30),
        }
    }
}

/// The shared mechanism: pump the default main context until `done()` is true or
/// `deadline` elapses, and report whether it converged.
///
/// GTK4Rs/AP-79: the bound is a timeout **source** installed *before* the loop, so a
/// blocking `iteration(true)` with nothing else pending is still guaranteed to
/// return by `deadline` rather than parking forever. `done()` is checked before the
/// watchdog flag on every pass, so a condition that becomes true on the very turn the
/// watchdog also fires still counts as converged, not timed out.
fn pump(deadline: Duration, mut done: impl FnMut() -> bool) -> bool {
    let ctx = glib::MainContext::default();
    let fired = Rc::new(Cell::new(false));
    let f = Rc::clone(&fired);
    let watchdog = glib::timeout_add_local_once(deadline, move || f.set(true));
    let mut watchdog = Some(watchdog);
    let converged = loop {
        if done() {
            break true;
        }
        if fired.get() {
            break false;
        }
        ctx.iteration(true);
    };
    if let Some(id) = watchdog.take() {
        if !fired.get() {
            id.remove();
        }
    }
    converged
}

/// Pump until `done()` holds, panicking with `what` if `clock`'s default deadline
/// elapses first. Prefer this over [`until_or_for`] wherever a timeout is always a
/// test bug (the common case) — a silently-ignored timeout lets the assertion that
/// follows fail for the wrong reason instead of naming what it was actually waiting
/// for.
pub(crate) fn until(clock: Clock, what: &str, done: impl FnMut() -> bool) {
    until_for(clock, clock.default_deadline(), what, done)
}

/// [`until`], with an explicit deadline instead of `clock`'s default — for a call
/// site with a sharper bound already measured (e.g. "inotify lands in ~60 ms, so 1 s
/// is generous, not a guess").
pub(crate) fn until_for(clock: Clock, deadline: Duration, what: &str, done: impl FnMut() -> bool) {
    assert!(
        pump(deadline, done),
        "pump watchdog ({deadline:?}, {clock:?}) fired waiting for: {what}"
    );
}

/// Non-panicking, explicit-deadline form of [`until`]: pumps until `done()` or
/// `deadline`, and reports whether it converged. Reach for this only where a caller
/// genuinely branches on convergence rather than always treating a timeout as a bug —
/// every current call site also has its own pre-migration deadline to preserve, which
/// is why there is no bare `until_or` taking `clock`'s default: add one back the day a
/// call site actually wants it (YAGNI, not an oversight).
pub(crate) fn until_or_for(_clock: Clock, deadline: Duration, done: impl FnMut() -> bool) -> bool {
    pump(deadline, done)
}

/// Drain the main loop for a fixed SPAN, with no completion predicate at all —
/// for a call site that needs the loop to run for a while before it has any
/// observable state of its own to poll (e.g. "give the async clipboard read a
/// chance to be dispatched, then go check the buffer"). Unlike [`until`]/[`until_or`],
/// `deadline` here is not a failure bound, it IS the wait — this always consumes the
/// full span, so pick the tightest one the awaited work actually needs. The same
/// shape as `window/rename.rs::pump_for` (left there, not migrated — see the module
/// rustdoc), generalised with an explicit clock for the sites that do route through
/// this module.
pub(crate) fn drain_for(_clock: Clock, deadline: Duration) {
    pump(deadline, || false);
}
