//! The cancel-then-release lifetime of a document's live-reload `GFileMonitor`.

use gtk::gio;
use gtk::prelude::*;

/// A document's live-reload file monitor, owned so that **a cancelled monitor
/// cannot outlive the statement that cancelled it**.
///
/// Finalizing a *cancelled* `GFileMonitor` after the main context has dispatched
/// aborts the process with `STATUS_HEAP_CORRUPTION` on Windows — no panic, no GLib
/// warning, no failing assertion (ScrAP-297). The safe ordering is cancel and
/// release in one uninterrupted stretch, which is what both call sites already did;
/// nothing enforced it, so any future code that parked a second reference across a
/// main-loop turn would reintroduce a silent, Windows-only process kill invisible to
/// the other two seats.
///
/// This type is deliberately **not `Clone`**: there is no way to obtain a second
/// owned reference, so the hazardous ordering is unrepresentable rather than
/// remembered. [`cancel_and_release`](Self::cancel_and_release) consumes `self`, so
/// a cancelled monitor is not a value a caller still holds.
///
/// **Precondition on the `changed` callback:** it must not capture its own clone of
/// anything that would keep the monitor alive; the monitor's lifetime is this value.
pub(crate) struct DocMonitor {
    inner: gio::FileMonitor,
}

impl DocMonitor {
    /// Watch `file` for changes. The only constructor.
    ///
    /// `None` means GIO refused the watch — the caller has no monitor, which on
    /// every platform means live reload is simply inactive for that document.
    pub(crate) fn attach(file: &gio::File) -> Option<Self> {
        // The sole sanctioned `FileExt::monitor_file` in the tree.
        #[allow(clippy::disallowed_methods)]
        file.monitor_file(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE)
            .ok()
            .map(|inner| Self { inner })
    }

    /// Wire the monitor's `changed` handler.
    ///
    /// Takes `&self`, so the handler is installed without handing out an owned
    /// reference the caller could park.
    pub(crate) fn connect_changed<F>(&self, f: F)
    where
        F: Fn(gio::FileMonitorEvent) + 'static,
    {
        self.inner.connect_changed(move |_, _, _, event| f(event));
    }

    /// Cancel the monitor and release it in the same statement.
    ///
    /// Consuming `self` is the point: no main-loop turn can intervene between the
    /// cancel and the final unref, which is the ordering GIO mishandles (ScrAP-297).
    pub(crate) fn cancel_and_release(self) {
        // The sole sanctioned `FileMonitorExt::cancel` in the tree.
        #[allow(clippy::disallowed_methods)]
        self.inner.cancel();
        drop(self);
    }

    /// A handle for observing this monitor's state *after* production code has
    /// cancelled and released it. Test-only, and structurally leaky by design.
    ///
    /// See [`ObservationHandle`] — the leak is what makes the observation safe.
    #[cfg(test)]
    pub(crate) fn observation_handle(&self) -> ObservationHandle {
        ObservationHandle(std::mem::ManuallyDrop::new(self.inner.clone()))
    }
}

/// A never-released reference to a monitor, for a test that must read its state
/// after production code has cancelled it.
///
/// The reference is held in a `ManuallyDrop` and is **never** unrefed: releasing it
/// is precisely the ScrAP-297 abort, since by the time a test can observe the
/// cancelled state the main context has necessarily dispatched. Leaking one GObject
/// per test run is the price of asserting on the real monitor instead of on
/// something that merely finalizes safely.
///
/// The leak is a property of the type, not of a line a later reader might tidy away.
#[cfg(test)]
pub(crate) struct ObservationHandle(std::mem::ManuallyDrop<gio::FileMonitor>);

#[cfg(test)]
impl ObservationHandle {
    /// Whether the monitor has been cancelled.
    ///
    /// `is_cancelled()`, never `property::<bool>("cancelled")`: below GLib 2.84 the
    /// property getter is hard-coded `FALSE` (`gfilemonitor.c:105-108`), so on this
    /// project's own 2.72.4 floor the property would answer "live" for a cancelled
    /// monitor and invert every assertion built on it.
    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}

#[cfg(test)]
mod tests {
    use super::DocMonitor;
    use gtk::gio;
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    /// A monitor over a real file attaches, reports live, and reports cancelled once
    /// released — the whole lifetime the seam exists to make ordered.
    ///
    /// The observation handle is taken *before* the release for the same reason the
    /// rename integration test does it: `cancel_and_release` consumes the monitor, so
    /// afterwards there is no value left to ask (ScrAP-297).
    #[test]
    fn a_monitor_is_live_until_it_is_cancelled_and_released() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("watched.md");
        std::fs::write(&path, "# body\n").unwrap();

        let monitor = DocMonitor::attach(&gio::File::for_path(&path))
            .expect("GIO watches an existing regular file");
        let observed = monitor.observation_handle();
        assert!(!observed.is_cancelled(), "a fresh monitor is live");

        monitor.cancel_and_release();
        assert!(
            observed.is_cancelled(),
            "cancel_and_release must cancel, not merely drop"
        );
    }

    /// The `changed` adapter delivers GIO's event to a handler that takes only the
    /// event — the shape the seam narrows the callback to, so no caller can capture
    /// the monitor from its own handler.
    #[test]
    fn the_changed_adapter_delivers_an_event() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("watched.md");
        std::fs::write(&path, "# body\n").unwrap();

        // A context this test OWNS, pushed as thread-default before the monitor is
        // attached so the monitor's source lands on it. The default context is not
        // usable here: a `#[gtktest::test]` body earlier in the same binary leaves it
        // acquired by the harness's serializing thread, and `iteration` on a thread
        // that does not own the context dispatches nothing at all — so the handler
        // never runs and the 5 s deadline burns. That failure is invisible on Linux,
        // where whole-suite ordering happens to leave the default context free, and
        // deterministic on Windows.
        let ctx = glib::MainContext::new();
        ctx.with_thread_default(|| {
            let monitor = DocMonitor::attach(&gio::File::for_path(&path)).unwrap();
            let seen = Rc::new(Cell::new(false));
            let seen_c = Rc::clone(&seen);
            monitor.connect_changed(move |_event| seen_c.set(true));

            // The watch is established on GLib's private worker thread, so a same-turn
            // write is never seen — this needs WALL CLOCK, not a count of iterations
            // (GTK4Rs/AP-261: turns are not time).
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline && !seen.get() {
                std::fs::write(&path, format!("# body {:?}\n", Instant::now())).unwrap();
                std::thread::sleep(Duration::from_millis(50));
                while ctx.iteration(false) {}
            }
            assert!(
                seen.get(),
                "a write to the watched file reaches the handler"
            );

            monitor.cancel_and_release();
        })
        .expect("this thread can take a fresh context as its thread-default");
    }
}
