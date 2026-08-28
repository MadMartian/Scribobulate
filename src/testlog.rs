//! Test-only capture of the `log` facade, so a test can assert that something was
//! **logged** and not merely that it was absent.
//!
//! # Why this exists
//!
//! A decoration in this project's vocabulary is *inert by default*: every refusal
//! degrades to "this decoration is absent". That makes "the theme stated nothing",
//! "the reference resolved nowhere" and "the file would not decode" produce identical
//! pixels — so a pixel assertion cannot tell a diagnosed failure from a silent one,
//! and the only observable that distinguishes them is the log record (ScrAP-324).
//! `sdd/POLICY.md` § Logging makes that record part of the contract; nothing could
//! test it.
//!
//! # Process-global state, restored by construction
//!
//! `log` permits exactly one global logger per process and has no way to remove it,
//! so this module installs a permanent, *idle* logger the first time a test asks for
//! one. Idle it forwards nothing and records nothing. [`capture`] hands out an RAII
//! guard that turns recording on, and turning it off again is the guard's `Drop` —
//! which also releases the mutex the guard holds, so two capturing tests cannot
//! interleave and no early return or panic can leave the sink armed
//! (`sdd/POLICY.md` § Unit tests).
//!
//! Tests never call `crate::logging::init`, so this does not contend with the
//! application's own sink; if some future test did, `set_logger` would refuse and
//! [`capture`] would report an empty log rather than lying about one.
//!
//! # Scoped to the capturing test's own thread
//!
//! The guard serialises capturing tests against each other, but libtest keeps running
//! every OTHER test in parallel and those tests log too — into the one armed sink. An
//! assertion of PRESENCE survives that; an assertion of ABSENCE, or a COUNT, does not,
//! and those are exactly the assertions this module exists to make possible. So the
//! sink records only what arrives on the thread that took the guard, which libtest
//! gives each test exclusively. The cost is stated rather than discovered: a record
//! emitted by code under test from a WORKER thread is not captured — this project
//! spawns none of its own (`sdd/POLICY.md` § Architecture rules), and a future test
//! whose subject logs off-thread needs a different scope, not a wider one here.

use std::sync::{Mutex, MutexGuard, OnceLock};
use std::thread::ThreadId;

/// One captured record, flattened to the two things an assertion cares about.
#[derive(Clone, Debug)]
pub(crate) struct Record {
    pub(crate) level: log::Level,
    pub(crate) message: String,
}

/// The records captured since the live [`Capture`] guard was taken, and the thread
/// entitled to write into it. `None` when no guard is live, which is what makes the
/// idle logger free.
static SINK: Mutex<Option<(ThreadId, Vec<Record>)>> = Mutex::new(None);

/// Serialises capturing tests against each other. Held for the guard's lifetime.
fn lock() -> &'static Mutex<()> {
    static L: OnceLock<Mutex<()>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(()))
}

struct Capturing;

impl log::Log for Capturing {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        // `lock()` is held by the guard, not here: a `log!` from inside the capturing
        // test would deadlock on it. The sink's own mutex is enough — it is taken and
        // released within this call.
        if let Ok(mut sink) = SINK.lock() {
            if let Some((owner, records)) = sink.as_mut() {
                if *owner == std::thread::current().id() {
                    records.push(Record {
                        level: record.level(),
                        message: record.args().to_string(),
                    });
                }
            }
        }
    }

    fn flush(&self) {}
}

/// Everything `log` emitted while this guard was alive.
///
/// Deliberately not `Deref<Target = [Record]>`: [`Capture::records`] takes a fresh
/// snapshot each call, so a test may assert, log more, and assert again.
pub(crate) struct Capture {
    _serialised: MutexGuard<'static, ()>,
}

impl Capture {
    /// A snapshot of the records captured so far.
    pub(crate) fn records(&self) -> Vec<Record> {
        SINK.lock()
            .ok()
            .and_then(|s| s.as_ref().map(|(_, records)| records.clone()))
            .unwrap_or_default()
    }

    /// Whether any captured record at `level` contains `needle`.
    ///
    /// A substring match rather than an equality one on purpose: the assertion a
    /// caller wants is "this refusal was diagnosed", and pinning a whole sentence
    /// makes every wording change a test failure without making the guard stronger.
    pub(crate) fn logged(&self, level: log::Level, needle: &str) -> bool {
        self.records()
            .iter()
            .any(|r| r.level == level && r.message.contains(needle))
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        if let Ok(mut sink) = SINK.lock() {
            *sink = None;
        }
    }
}

/// Begin capturing `log` records, serialised against every other capturing test.
///
/// The returned guard must be held for as long as the code under test runs; dropping
/// it disarms the sink and releases the lock.
pub(crate) fn capture() -> Capture {
    let serialised = lock().lock().unwrap_or_else(|e| e.into_inner());
    // Installed once per process and never removed — see the module header. A second
    // call, or a process that already installed the application sink, gets `Err` and
    // the capture stays empty rather than silently attaching to the wrong logger.
    static INSTALLED: OnceLock<bool> = OnceLock::new();
    if *INSTALLED.get_or_init(|| log::set_logger(&Capturing).is_ok()) {
        log::set_max_level(log::LevelFilter::Trace);
    }
    if let Ok(mut sink) = SINK.lock() {
        *sink = Some((std::thread::current().id(), Vec::new()));
    }
    Capture {
        _serialised: serialised,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The positive control for every test that uses this module: if the sink cannot
    /// see a record it was definitely handed, an assertion of *absence* elsewhere
    /// means nothing.
    #[test]
    fn a_captured_warning_is_visible_and_a_silent_block_produces_nothing() {
        let cap = capture();
        log::warn!("testlog: a distinctive marker 8f2a");
        assert!(cap.logged(log::Level::Warn, "distinctive marker 8f2a"));
        // Level and content are both discriminating, so a test asserting "warned"
        // cannot be satisfied by an unrelated record at another level.
        assert!(!cap.logged(log::Level::Error, "distinctive marker 8f2a"));
        assert!(!cap.logged(log::Level::Warn, "a marker nothing emits"));
    }

    /// The sink is the capturing test's alone — the property every ABSENCE and every
    /// COUNT assertion elsewhere rests on. Without it, a concurrently running test
    /// that happens to log the same substring turns a correct guard red at random.
    #[test]
    fn another_threads_records_never_enter_the_capture() {
        let cap = capture();
        std::thread::spawn(|| log::warn!("testlog: from a foreign thread 3b71"))
            .join()
            .expect("the probe thread completes");
        log::warn!("testlog: from the owning thread 3b71");
        assert!(cap.logged(log::Level::Warn, "from the owning thread 3b71"));
        assert!(!cap.logged(log::Level::Warn, "from a foreign thread 3b71"));
    }
}
