//! The logging sink: one `log::Log` that fans a record out to three places.
//!
//! ```text
//!                      ┌─ the breadcrumb ring  (in memory, read by the crash handler)
//!   log::info!(…) ─────┼─ the persistent file  (state dir, survives the process)
//!                      └─ env_logger          (stderr, exactly as before)
//! ```
//!
//! Wrapping `env_logger`'s `Logger` rather than replacing it is what keeps TDD
//! 21.1's "stderr keeps behaving as it does today" true: the existing renderer and
//! the existing `RUST_LOG` filter are the same objects, reached through the same
//! code, and this type adds two consumers in front of them.
//!
//! # The threshold, and why it is not simply `RUST_LOG`
//!
//! The recorded crashes died with a log whose last line was *hours* stale, because
//! the default filter is `warn` and nothing in the app logs a lifecycle event above
//! `debug`. Breadcrumbs therefore cannot be gated on `RUST_LOG` — the whole point is
//! that they are already being recorded when the crash nobody expected arrives.
//!
//! So the recording threshold is `max(RUST_LOG, Info)`: `info` and worse are always
//! recorded, and anything `RUST_LOG` additionally asks for joins them. The `max` is
//! load-bearing in the other direction too (TDD 21.11) — it leaves `debug`/`trace`
//! **off** by default, so the per-frame hot paths keep costing a branch and no
//! formatting, which is the property POLICY § Logging asks call sites to rely on.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use log::{Level, LevelFilter, Log, Metadata, Record};

use super::ring::Ring;

/// Size cap for the live log file. One rotation is kept beside it, so the forensic
/// log costs at most `2 × MAX_LOG_BYTES` on disk, plus at most
/// [`MAX_ROTATE_FAILURES`] further records (TDD 21.2).
///
/// **The overshoot term is not pedantry — without it the bound was false** (QA round 5,
/// F-SEC5-008). Rotation failure used to reset `written` to 0 and keep appending, so
/// `should_rotate` measured the next megabyte from zero rather than from the file's true
/// size and every failed rotation bought another `MAX_LOG_BYTES` with no ceiling. The
/// stated `2 ×` held only on the success path, which is the same shape as a doc comment
/// asserting a guarantee nothing checks.
///
/// 1 MiB is roughly 8000 lifecycle records — days of ordinary use, and still small
/// enough to attach to a bug report whole.
pub(crate) const MAX_LOG_BYTES: u64 = 1024 * 1024;

/// How many consecutive rotation failures are tolerated before file logging is given
/// up for this run.
///
/// Retrying is right: rotation fails for reasons that clear (a directory momentarily
/// read-only), and `written` now keeps its true value so every subsequent write
/// re-attempts the rename rather than silently granting another megabyte. Retrying
/// *forever* is not: it is a `rename` syscall per record against a directory that has
/// already refused, and the file grows by a record each time.
///
/// So after this many attempts the sink drops to ring + stderr. That is what makes the
/// disk bound TRUE rather than aspirational, and it keeps the rule the file logger was
/// written under — a diagnostic aid must never stop the application, and it has not:
/// breadcrumbs and stderr are untouched, which are the two the crash report reads from.
const MAX_ROTATE_FAILURES: u32 = 8;

/// Suffix of the single retained previous log.
const ROTATED_SUFFIX: &str = ".1";

/// The live log's file name inside the state directory.
pub(crate) const LOG_FILE_NAME: &str = "scribobulate.log";

/// The open log file and how much has been written to it, so rotation is decided
/// without a `stat` per record.
struct LogFile {
    path: PathBuf,
    handle: File,
    written: u64,
    /// Consecutive failed rotations — see [`MAX_ROTATE_FAILURES`].
    rotate_failures: u32,
}

/// The process-wide `log::Log`.
pub(crate) struct ForensicSink {
    inner: env_logger::Logger,
    ring: &'static Ring,
    /// `None` when no state directory could be located, or the file could not be
    /// opened. Logging then degrades to today's behaviour rather than failing —
    /// a diagnostic aid must never be able to stop the application starting.
    file: Mutex<Option<LogFile>>,
}

impl ForensicSink {
    /// Build the sink, opening (and if necessary rotating) the log file and writing
    /// `header` — the identity block — as the first thing in this run's records.
    pub(crate) fn new(
        inner: env_logger::Logger,
        ring: &'static Ring,
        log_path: Option<&Path>,
        header: &str,
    ) -> Self {
        let file = log_path.and_then(|path| open_log(path, header));
        Self {
            inner,
            ring,
            file: Mutex::new(file),
        }
    }

    /// Record a line into the ring and the persistent log while consulting **no**
    /// level filter at all.
    ///
    /// The single sanctioned bypass, and it exists for one caller:
    /// `logging::forward`'s benign-GTK-noise demotion. That demotion is a
    /// *display* decision — "do not alarm someone reading a default startup log" —
    /// and applying it to the forensic record would be a category error. It is also
    /// concretely wrong: the benign gizmo transient and the GTK4Rs/AP-30 popover warning
    /// were the *only* context either 2026-07-29 crash left behind, so the records
    /// most likely to be filtered are the ones the plan says to keep.
    ///
    /// Only `logging::forward` may call this. Everything else must go through the
    /// `log` macros, where the threshold applies — an opt-in "also record this" API
    /// used freely would be a latent gap by construction (GTK4Rs/AP-108's enforcement
    /// lesson), which is why it is named for its one purpose rather than generally.
    pub(crate) fn record_demoted_diagnostic(&self, level: Level, target: &str, message: &str) {
        let line = format_line(&super::timefmt::now_iso8601(), level, target, message);
        self.ring.record(&line);
        self.write_file(&line);
    }

    /// The level below which nothing is recorded, given `RUST_LOG`'s own filter.
    pub(crate) fn recording_filter(env_filter: LevelFilter) -> LevelFilter {
        env_filter.max(LevelFilter::Info)
    }

    /// Whether this record is recorded (ring + file), as opposed to merely printed.
    fn records(&self, record: &Record<'_>) -> bool {
        record.level() <= Level::Info || self.inner.matches(record)
    }

    /// Append one already-formatted line to the persistent log, rotating first if
    /// it would push the file past [`MAX_LOG_BYTES`].
    ///
    /// Unbuffered: one `write(2)` per record, no `BufWriter`. A buffered tail is
    /// precisely what a SIGSEGV discards, and discarding the last few records is
    /// discarding the ones that matter.
    fn write_file(&self, line: &str) {
        let mut guard = self
            .file
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(file) = guard.as_mut() else {
            return;
        };
        let bytes = line.len() as u64 + 1;
        if should_rotate(file.written, bytes, MAX_LOG_BYTES) {
            match rotate(&file.path) {
                Some(reopened) => *file = reopened,
                None => {
                    // Rotation failed (a read-only state dir, say). Keep writing to the
                    // file we have — an oversized log beats a silent one — but keep
                    // `written` at its TRUE value, so the next record re-attempts the
                    // rename instead of being granted a fresh MAX_LOG_BYTES. Resetting
                    // it to 0 here is what made the `2 ×` bound false (F-SEC5-008).
                    file.rotate_failures += 1;
                    if file.rotate_failures >= MAX_ROTATE_FAILURES {
                        // Persistently refused. Give the file up rather than issue a
                        // rename per record forever against a directory that has said
                        // no eight times; the ring and stderr carry on.
                        *guard = None;
                        return;
                    }
                }
            }
        }
        if writeln!(file.handle, "{line}").is_ok() {
            file.written += bytes;
        }
    }
}

impl Log for ForensicSink {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= Level::Info || self.inner.enabled(metadata)
    }

    fn log(&self, record: &Record<'_>) {
        if self.records(record) {
            let line = format_line(
                &super::timefmt::now_iso8601(),
                record.level(),
                record.target(),
                &record.args().to_string(),
            );
            self.ring.record(&line);
            self.write_file(&line);
        }
        // `env_logger::Logger::log` applies its own `RUST_LOG` filter, so stderr
        // sees exactly what it saw before this type existed — the extra consumers
        // above cannot widen it.
        self.inner.log(record);
    }

    fn flush(&self) {
        self.inner.flush();
        let mut guard = self
            .file
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(file) = guard.as_mut() {
            let _ = file.handle.flush();
        }
    }
}

/// One record, rendered for both the ring and the file.
///
/// `{timestamp} {LEVEL} {target}: {message}` — the target is the module path by
/// POLICY's call-site convention, so a breadcrumb names *where* as well as *what*
/// with no extra call-site work. Newlines are folded to `⏎` so one record is always
/// one line: a multi-line GTK critical must not be able to fake several breadcrumbs
/// or split a ring entry across slots.
pub(crate) fn format_line(timestamp: &str, level: Level, target: &str, message: &str) -> String {
    let folded = if message.contains(['\n', '\r']) {
        message.replace(['\n', '\r'], "⏎")
    } else {
        message.to_owned()
    };
    format!("{timestamp} {level:<5} {target}: {folded}")
}

/// Whether appending `incoming` bytes to a file already holding `written` should
/// rotate first.
///
/// Pure, so the boundary is testable without writing a megabyte. `written > 0`
/// keeps a single record larger than the cap from rotating an empty file forever
/// (it is written oversized instead, and rotates on the next record).
pub(crate) fn should_rotate(written: u64, incoming: u64, cap: u64) -> bool {
    written > 0 && written + incoming > cap
}

/// The retained-previous-log path for a live log path.
pub(crate) fn rotated_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(ROTATED_SUFFIX);
    path.with_file_name(name)
}

/// Rename the live log aside and reopen an empty one. `None` if either step fails.
fn rotate(path: &Path) -> Option<LogFile> {
    // Overwrites any existing `.1`: exactly one generation is retained, which is
    // what bounds the total (TDD 21.2).
    std::fs::rename(path, rotated_path(path)).ok()?;
    // The renamed `.1` carries whatever mode the live log had — including a legacy
    // 0664 from a build that used `File::create` (F-SEC5-007).
    #[cfg(unix)]
    super::make_path_private(&rotated_path(path));
    let handle = super::private_options()
        .create(true)
        .append(true)
        .open(path)
        .ok()?;
    Some(LogFile {
        path: path.to_path_buf(),
        handle,
        written: 0,
        rotate_failures: 0,
    })
}

/// Open (creating the directory if needed) and stamp the log with `header`.
fn open_log(path: &Path, header: &str) -> Option<LogFile> {
    if let Some(parent) = path.parent() {
        crate::session::create_state_dir(parent).ok()?;
    }
    let mut written = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    // Rotate at *open* as well as on write, so a log left at the cap by the previous
    // run does not start this one already over it.
    if written >= MAX_LOG_BYTES {
        let _ = std::fs::rename(path, rotated_path(path));
        #[cfg(unix)]
        super::make_path_private(&rotated_path(path));
        written = 0;
    }
    let mut handle = super::private_options()
        .create(true)
        .append(true)
        .open(path)
        .ok()?;
    // `private_options`'s mode applies only when the open CREATES the file, and this
    // path is stable across runs — so a log left by an older build stays at its old
    // mode forever unless it is asserted on the handle (F-SEC5-007). The document-save
    // path already does exactly this; `atomic_io.rs` is the precedent.
    #[cfg(unix)]
    super::make_private(&handle);
    let banner = format!("\n=== run start ===\n{header}");
    if handle.write_all(banner.as_bytes()).is_ok() {
        written += banner.len() as u64;
    }
    Some(LogFile {
        path: path.to_path_buf(),
        handle,
        written,
        rotate_failures: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sink(dir: &Path) -> ForensicSink {
        ForensicSink::new(
            env_logger::Builder::new().build(),
            Box::leak(Box::new(Ring::new())),
            Some(&dir.join(LOG_FILE_NAME)),
            "scribobulate 0.1.0 (test)\n",
        )
    }

    #[test]
    fn the_recording_threshold_is_never_quieter_than_info() {
        // TDD 21.3: breadcrumbs are recorded with no switch turned on. The default
        // filter is `warn`, so without this `max` a lifecycle event would be lost —
        // which is exactly how the recorded crashes came to have a stale log.
        assert_eq!(
            ForensicSink::recording_filter(LevelFilter::Warn),
            LevelFilter::Info
        );
        assert_eq!(
            ForensicSink::recording_filter(LevelFilter::Off),
            LevelFilter::Info
        );
        // …and never louder than asked for, so `trace` hot paths stay off (21.11).
        assert_eq!(
            ForensicSink::recording_filter(LevelFilter::Trace),
            LevelFilter::Trace
        );
    }

    #[test]
    fn a_record_renders_with_its_time_level_and_module() {
        let line = format_line(
            "2026-07-29T01:54:39.123Z",
            Level::Info,
            "scribobulate::app::open",
            "opened /tmp/a.md",
        );
        assert_eq!(
            line,
            "2026-07-29T01:54:39.123Z INFO  scribobulate::app::open: opened /tmp/a.md"
        );
    }

    #[test]
    fn a_multi_line_message_stays_one_record() {
        // A GTK critical arrives with embedded newlines; unfolded it would split a
        // ring slot and forge extra breadcrumbs.
        let line = format_line("T", Level::Error, "Gtk", "first\nsecond\r\nthird");
        assert!(!line.contains('\n') && !line.contains('\r'), "{line:?}");
        assert!(line.ends_with("first⏎second⏎⏎third"));
    }

    #[test]
    fn rotation_triggers_only_at_the_cap_and_never_on_an_empty_file() {
        assert!(
            !should_rotate(0, 5_000, 1_000),
            "empty file must not rotate"
        );
        assert!(
            !should_rotate(900, 100, 1_000),
            "exactly at the cap is fine"
        );
        assert!(should_rotate(901, 100, 1_000));
    }

    #[test]
    fn the_rotated_name_sits_beside_the_live_one() {
        assert_eq!(
            rotated_path(Path::new("/state/scribobulate/scribobulate.log")),
            Path::new("/state/scribobulate/scribobulate.log.1")
        );
    }

    #[test]
    fn opening_writes_the_identity_header_first() {
        // TDD 21.1: the first records of every run name the build.
        let dir = tempfile::tempdir().unwrap();
        let _sink = sink(dir.path());
        let text = std::fs::read_to_string(dir.path().join(LOG_FILE_NAME)).unwrap();
        assert!(text.contains("=== run start ==="));
        assert!(text.contains("scribobulate 0.1.0 (test)"));
    }

    #[test]
    fn a_record_reaches_both_the_ring_and_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let sink = sink(dir.path());
        sink.log(
            &Record::builder()
                .args(format_args!("opened /tmp/a.md"))
                .level(Level::Info)
                .target("scribobulate::app::open")
                .build(),
        );
        let text = std::fs::read_to_string(dir.path().join(LOG_FILE_NAME)).unwrap();
        assert!(text.contains("opened /tmp/a.md"), "{text}");
        assert!(sink.ring.snapshot().iter().any(|l| l.contains("opened")));
    }

    #[test]
    fn an_info_record_is_recorded_even_though_stderr_is_at_warn() {
        // The default `RUST_LOG` filter is `warn`; an empty builder matches it.
        // This is the exact gap that left the recorded crashes context-free.
        let dir = tempfile::tempdir().unwrap();
        let sink = sink(dir.path());
        assert!(sink.enabled(&Metadata::builder().level(Level::Info).build()));
        sink.log(
            &Record::builder()
                .args(format_args!("lifecycle event"))
                .level(Level::Info)
                .target("scribobulate")
                .build(),
        );
        assert!(std::fs::read_to_string(dir.path().join(LOG_FILE_NAME))
            .unwrap()
            .contains("lifecycle event"));
    }

    #[test]
    fn a_debug_record_is_not_recorded_by_default() {
        // TDD 21.11: hot paths stay off unless `RUST_LOG` asks for them.
        let dir = tempfile::tempdir().unwrap();
        let sink = sink(dir.path());
        assert!(!sink.enabled(&Metadata::builder().level(Level::Debug).build()));
    }

    #[test]
    fn the_log_stays_bounded_and_keeps_the_newest_records() {
        // TDD 21.2 end to end: write past the cap and assert both halves — the live
        // file holds the *recent* records, and only one generation is retained.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LOG_FILE_NAME);
        let sink = ForensicSink::new(
            env_logger::Builder::new().build(),
            Box::leak(Box::new(Ring::new())),
            Some(&path),
            "header\n",
        );
        for n in 0..40_000 {
            sink.write_file(&format!("{n:07} filler filler filler filler"));
        }
        let live = std::fs::read_to_string(&path).unwrap();
        assert!(live.len() as u64 <= MAX_LOG_BYTES + 4_096);
        assert!(
            live.contains("0039999 filler"),
            "newest record must survive"
        );
        assert!(rotated_path(&path).exists(), "one generation is retained");
        assert!(
            !dir.path().join("scribobulate.log.2").exists(),
            "only one generation is retained"
        );
    }

    #[test]
    fn an_unusable_state_directory_degrades_instead_of_failing() {
        // A diagnostic aid must never stop the app starting.
        let sink = ForensicSink::new(
            env_logger::Builder::new().build(),
            Box::leak(Box::new(Ring::new())),
            None,
            "header\n",
        );
        sink.log(
            &Record::builder()
                .args(format_args!("still recorded in memory"))
                .level(Level::Info)
                .target("scribobulate")
                .build(),
        );
        assert!(sink.ring.snapshot().iter().any(|l| l.contains("still")));
    }
    /// A log this build did NOT create is forced private (QA round 5, F-SEC5-007).
    ///
    /// **The population the existing permission test cannot range over.**
    /// `report.rs`'s two-write test proves the *second writer* does not relax the mode,
    /// but both of its writes go through `private_options()` on a path it created
    /// itself. `open(2)`'s mode applies only at creation, and the log path is stable
    /// across runs — no timestamp, no pid — so a `scribobulate.log` left behind by a
    /// build that used `File::create` is already 0664 under the common umask and stays
    /// that way forever. Every upgrading user, indefinitely.
    ///
    /// So this test creates the file the way the OLD build did, then opens it the way
    /// the new one does. Mutation guard: remove `make_private` from `open_log` and this
    /// fails with the pre-fix mode.
    #[test]
    fn a_log_left_behind_by_an_older_build_is_made_private() {
        #[cfg(not(unix))]
        {
            crate::testsymlink::skipped(
                "TDD 21.12 log permissions",
                "file modes are a unix concept",
            );
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join(LOG_FILE_NAME);

            // Exactly what the pre-seam build did: no mode stated at all.
            std::fs::File::create(&path).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o664)).unwrap();
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o664,
                "precondition: a legacy log is group/world readable"
            );

            let _sink = sink(dir.path());
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                super::super::PRIVATE_FILE_MODE,
                "an existing forensic log must be narrowed on open — the creation mode \
                 cannot reach a file this build did not create"
            );
        }
    }

    /// The `2 × MAX_LOG_BYTES` bound survives a state directory that refuses renames
    /// (QA round 5, F-SEC5-008).
    ///
    /// Rotation failure used to reset `written` to 0 and keep appending, so
    /// `should_rotate` measured the next megabyte from zero and each failure bought
    /// another `MAX_LOG_BYTES` with no ceiling — the doc's `2 ×` held only on the
    /// success path. Mutation guard: restore `None => file.written = 0` and the file
    /// grows past the assertion below.
    #[test]
    fn a_log_that_cannot_rotate_stays_bounded() {
        #[cfg(not(unix))]
        {
            crate::testsymlink::skipped(
                "TDD 21.2 log bound under rotation failure",
                "the failure is induced with POSIX directory permissions",
            );
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join(LOG_FILE_NAME);
            let sink = sink(dir.path());

            // Push the live log to the cap while renames still work.
            let filler = "x".repeat(4096);
            let mut written = 0u64;
            while written < MAX_LOG_BYTES {
                sink.log(
                    &Record::builder()
                        .args(format_args!("{filler}"))
                        .level(Level::Info)
                        .target("scribobulate")
                        .build(),
                );
                written += filler.len() as u64 + 1;
            }

            // Now forbid renames: the open handle keeps working, `rename` does not.
            std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
            // Enough to cover another WHOLE MAX_LOG_BYTES. The count is load-bearing:
            // with the pre-fix `written = 0` reset, the next rotation attempt is only
            // triggered after another full cap's worth of writes, so a short loop stays
            // under the bound and the guard passes over the bug. MEASURED: at 32 records
            // this test survived the mutation it exists to kill.
            let records_per_cap = (MAX_LOG_BYTES / (filler.len() as u64 + 1)) + 1;
            for _ in 0..(records_per_cap + u64::from(MAX_ROTATE_FAILURES)) {
                sink.log(
                    &Record::builder()
                        .args(format_args!("{filler}"))
                        .level(Level::Info)
                        .target("scribobulate")
                        .build(),
                );
            }
            // Restore, so the tempdir can be cleaned up.
            std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();

            let size = std::fs::metadata(&path).unwrap().len();
            let bound = MAX_LOG_BYTES + u64::from(MAX_ROTATE_FAILURES) * (filler.len() as u64 + 1);
            assert!(
                size <= bound,
                "a log that cannot rotate grew to {size} bytes, past the {bound} the \
                 doc comment promises — each failed rotation must re-attempt, not grant \
                 another MAX_LOG_BYTES"
            );
        }
    }
}
