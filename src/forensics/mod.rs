//! Crash forensics — what the application leaves behind when it dies.
//!
//! # Why this exists
//!
//! Five SIGSEGVs were recovered from the operator's kernel log across four weeks,
//! three distinct faulting addresses, two of them repeating. Every one of them left
//! *nothing* that named the fault or the activity leading to it, because six
//! independent holes lined up:
//!
//! | Hole | Closed by |
//! |---|---|
//! | No fatal-signal handler; a SIGSEGV wrote nothing at all | [`signal`] |
//! | No panic report, and `RUST_BACKTRACE` unset in a desktop launch | [`report::install_panic_hook`] |
//! | No core dump — apport refuses an unpackaged executable, on every Ubuntu machine | [`signal`] (the in-process substitute) |
//! | Logs persisted only incidentally, via whatever launched the app | [`sink`] |
//! | Nothing at `warn` described user activity, so a log could not reconstruct the sequence | [`ring`] + the `info!` lifecycle records |
//! | No build identity in the log | [`identity`] |
//!
//! Symbols are the one hole deliberately left open: ScrAP-141 records that Ubuntu
//! jammy ships none for the installed GTK 4.6.9 in either `-dbgsym` or debuginfod,
//! and 4 of the 5 recorded faults are *inside* GTK. A backtrace is therefore the
//! least valuable half of a report here, which is why it is written last and why
//! the effort went into the breadcrumb ring instead.
//!
//! # What a report costs when nothing goes wrong
//!
//! One `write(2)` per lifecycle record, an unconditional 224-byte copy into a fixed
//! ring, and 64 KiB of leaked alternate signal stack. No thread, no timer, no
//! allocation on the recording path beyond the formatted line itself, and
//! `debug`/`trace` stay off unless `RUST_LOG` asks (TDD 21.11).
//!
//! # Platform
//!
//! The fatal-signal half is `unix`-gated because POSIX signals do not exist
//! elsewhere — the same "not applicable off unix" justification as `workaround.rs`,
//! not "already dead" (TECH.md § platform-conditional code). Windows keeps the
//! persistent log, the breadcrumb ring, panic reports and the next-launch notice;
//! only the SIGSEGV report is missing, and a Win32 vectored-exception equivalent
//! would be the way to add it.

pub(crate) mod identity;
#[cfg(unix)]
pub(crate) mod rawbuf;
pub(crate) mod report;
pub(crate) mod ring;
#[cfg(unix)]
pub(crate) mod signal;
pub(crate) mod sink;
pub(crate) mod timefmt;

use std::path::PathBuf;

use ring::Ring;

/// The `OpenOptions` every forensic artifact must be created through — owner-only
/// (`0600`) on unix, platform default elsewhere.
///
/// **This is a seam, not a convenience.** A crash report and the persistent log both
/// carry breadcrumbs, window titles and session diagnostics, so both are
/// owner-private by intent. But the intent lived in exactly one of the four writers:
/// the fatal-signal handler passed `0600` to `open(2)` and every other creator used a
/// bare `File::create` / `File::options().create(true)`.
///
/// `open(2)`'s mode argument applies **only when the call creates the file**. So the
/// handler's `0600` is silently a no-op on a path that already exists — and the panic
/// hook writes the *same* report path through `report::write_report`. Panic first,
/// signal later, and the report the handler believed it had locked down keeps whatever
/// the panic hook's umask gave it. MEASURED, this machine: `File::create` then
/// `open(…, O_CREAT, 0600)` leaves the file at **0664** — group-writable and
/// world-readable, under a umask of 0002. Not 0644; whatever the umask says.
///
/// Neither writer is wrong on its own reading, which is why it survived review: one
/// states a mode and the other simply does not mention permissions. That is why the
/// fix is a shared constructor rather than a `.mode()` added at each site — a mode
/// that must be remembered at four call sites is a mode the fifth will omit
/// (GTK4Rs/AP-108/GTK4Rs/AP-130 enforcement reflex).
pub(crate) fn private_options() -> std::fs::OpenOptions {
    // Split per platform rather than mutating a shared binding: on Windows the `mode`
    // call below is compiled out, which left `mut` unused and failed the `-D warnings`
    // clippy gate on that platform only.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut opts = std::fs::OpenOptions::new();
        opts.mode(PRIVATE_FILE_MODE);
        opts
    }
    #[cfg(not(unix))]
    {
        std::fs::OpenOptions::new()
    }
}

/// The mode every forensic artefact is created with — **the constant, not just the
/// constructor** (QA round 5, L-7).
///
/// `private_options()` was introduced on the argument that "a mode that must be
/// remembered at four call sites is a mode the fifth will omit". Three writers routed
/// through it and a fourth — the fatal-signal handler's `open_report` — stated `0o600`
/// independently, because `OpenOptions::open` allocates and is not async-signal-safe, so
/// the handler must call `libc::open` directly. That constraint is real and is not the
/// defect: the defect is that the seam's own argument was left one call site short of
/// completion, and the site it missed is the one running while the process dies.
///
/// Naming the mode separately from the constructor lets the handler share the *value*
/// while keeping its own *call*, so the two cannot drift even though only one of them can
/// use `OpenOptions`. A seam is the constant plus the constructor, not the constructor
/// alone.
// Unix-only: every use is a POSIX mode call (`OpenOptionsExt::mode`, `from_mode`, the
// signal handler's `libc::open`). Ungated it is dead code on Windows and fails that
// platform's `-D warnings` clippy gate. The textual `super::`-reference assertion in
// `signal.rs` scans source text and is unaffected by this attribute.
#[cfg(unix)]
pub(crate) const PRIVATE_FILE_MODE: u32 = 0o600;

/// Force `file` to [`PRIVATE_FILE_MODE`] — for a path this build may not have created.
///
/// `open(2)`'s mode applies **only when the call creates the file** (the whole premise of
/// `private_options`). The log path is stable across runs — unlike a report path, it
/// carries no timestamp or pid — so a `scribobulate.log` left behind by an older build
/// that used `File::create` is already `0664` under a umask of `0002` and stays that way
/// forever, since every later open finds it existing (QA round 5, F-SEC5-007). The
/// document-save path already knew this and does exactly this follow-up
/// (`atomic_io.rs`'s fd-based `set_permissions`).
///
/// Best-effort by design: a diagnostic aid must never stop the application starting.
#[cfg(unix)]
pub(crate) fn make_private(file: &std::fs::File) {
    use std::os::unix::fs::PermissionsExt;
    let _ = file.set_permissions(std::fs::Permissions::from_mode(PRIVATE_FILE_MODE));
}

/// Path-based counterpart of [`make_private`], for a file this process does not hold open
/// — the rotated `.1` generation, which `rename` hands the live log's old mode.
#[cfg(unix)]
pub(crate) fn make_path_private(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(PRIVATE_FILE_MODE));
}

/// Largest index `<= limit` that is a UTF-8 character boundary in `s`.
///
/// **One copy, and the one that does not panic.** This existed twice — `ring::fit`
/// and `rawbuf::floor_char_boundary` — same algorithm, different bounds handling, and
/// the difference was a live hazard rather than a style difference (QA round 4 §3.2):
///
/// * `fit` early-returned when the limit covered the whole string, so its index was
///   always `< len`;
/// * `floor_char_boundary` clamped with `limit.min(s.len())` and then indexed
///   `bytes[i]` — which for `limit >= s.len()` is `bytes[len()]`, **out of bounds**.
///   MEASURED: `floor_char_boundary("héllo", 99)` panics with "the len is 6 but the
///   index is 6".
///
/// The clamp is the trap. It reads as the guard for exactly the case it creates, so a
/// caller checking whether the function is safe finds a `.min()` and stops looking.
/// Unreachable today — the only caller passes a limit strictly below the length — but
/// this is the crash-forensics module, where a panic runs while the process is already
/// dying and destroys the report it was writing. "Currently unreachable" is not a
/// property of this function, it is a property of its one caller.
///
/// Deduplicated here rather than into either module because `rawbuf` is `unix`-gated
/// and `ring` is not, so neither can depend on the other (ScrAP-219: put the shared
/// thing where every consumer can reach it, or it is not shared).
pub(crate) fn floor_char_boundary(s: &str, limit: usize) -> usize {
    if limit >= s.len() {
        return s.len();
    }
    let bytes = s.as_bytes();
    let mut i = limit;
    while i > 0 && bytes[i] & 0xC0 == 0x80 {
        i -= 1;
    }
    i
}

/// The process-wide breadcrumb ring.
///
/// A `static` rather than something owned by the sink, because the signal handler
/// must reach it without going through anything that can be locked or dropped.
static BREADCRUMBS: Ring = Ring::new();

/// The registered sink, kept reachable for the one path that must bypass the level
/// filter ([`record_demoted_diagnostic`]). `log`'s own `logger()` returns an opaque
/// `&dyn Log`, so the reference has to be kept here at install time or not at all.
static SINK: std::sync::OnceLock<&'static sink::ForensicSink> = std::sync::OnceLock::new();

/// Install the whole forensic kit and return the sink that must become the global
/// `log` implementation.
///
/// Called by [`crate::logging::init`], which owns registration — this module builds
/// the sink but never installs it, so there stays exactly one place in the tree
/// where the process's logger is decided (GTK4Rs/AP-18's discipline, applied to the
/// Rust side of the bridge as well as the glib side).
pub(crate) fn install(inner: env_logger::Logger) -> &'static sink::ForensicSink {
    let id = identity::collect();
    let header = identity::block(&id);
    let dir = state_directory();

    // Both files live beside `session.toml` in the state directory: they are
    // machine-local, regenerable, and must not follow a roaming profile.
    let log_path = dir.as_ref().map(|d| d.join(sink::LOG_FILE_NAME));
    let report_path = dir
        .as_ref()
        .map(|d| d.join(report::report_file_name(timefmt::now_unix_millis(), id.pid)));

    report::install_panic_hook(report_path.clone(), header.clone(), &BREADCRUMBS);

    #[cfg(unix)]
    if let Some(path) = report_path.as_deref() {
        signal::install(path, &header, &BREADCRUMBS);
    }

    // Leaked rather than boxed into `log`: the demotion bypass below needs a
    // `&'static` to the same sink, and `log::set_boxed_logger` would consume the
    // only handle to it.
    let sink: &'static sink::ForensicSink = Box::leak(Box::new(sink::ForensicSink::new(
        inner,
        &BREADCRUMBS,
        log_path.as_deref(),
        &header,
    )));
    let _ = SINK.set(sink);
    sink
}

/// Record a GTK/glib diagnostic that `logging::forward` demoted for display.
///
/// See [`sink::ForensicSink::record_demoted_diagnostic`] for why this exists and why
/// `logging::forward` is its only sanctioned caller. A no-op before the sink is
/// installed, which cannot happen in practice — the bridge is registered after it.
pub(crate) fn record_demoted_diagnostic(level: log::Level, target: &str, message: &str) {
    if let Some(sink) = SINK.get() {
        sink.record_demoted_diagnostic(level, target, message);
    }
}

/// Announce a crash report left by a previous run (TDD 21.9).
///
/// Separate from [`install`] and called after it, because the announcement is
/// itself a log record: it has to reach the sink that `install` built, and so
/// cannot happen while that sink is still being constructed.
pub(crate) fn announce_previous_crash() {
    if let Some(dir) = state_directory() {
        report::announce_unread_report(&dir);
    }
}

/// The application's state directory, or `None` if none can be located.
///
/// Delegates to [`crate::session::state_directory`] rather than resolving
/// `XDG_STATE_HOME` again: two lookups would be two chances to disagree, and the
/// session module already owns the platform fallbacks and the one-shot warning
/// (ScrAP-167).
fn state_directory() -> Option<PathBuf> {
    crate::session::state_directory()
}

#[cfg(test)]
mod tests {
    /// The process-wide ring is shared by the sink and the signal handler, so it
    /// must be reachable as a `&'static` from both. A compile-time check, which is
    /// the only kind that can fail usefully here.
    /// The bounds case the two duplicated copies disagreed about.
    ///
    /// `rawbuf`'s copy clamped with `limit.min(s.len())` and then indexed `bytes[i]`,
    /// so `limit >= s.len()` panicked — in the module that runs while the process is
    /// dying. It was unreachable via its one caller, which is a property of the caller
    /// and not of the function, so it is asserted here directly.
    #[test]
    fn a_limit_past_the_end_returns_the_length_instead_of_panicking() {
        assert_eq!(super::floor_char_boundary("héllo", 99), "héllo".len());
        assert_eq!(super::floor_char_boundary("", 0), 0);
        assert_eq!(super::floor_char_boundary("", 5), 0);
        // Still floors onto a boundary rather than splitting a multi-byte char:
        // "héllo" is h(1) é(2) l l o, so byte 2 is mid-`é` and floors to 1.
        assert_eq!(super::floor_char_boundary("héllo", 2), 1);
        assert_eq!(super::floor_char_boundary("héllo", 3), 3);
    }

    #[test]
    fn the_breadcrumb_ring_is_a_process_static() {
        let ring: &'static super::Ring = &super::BREADCRUMBS;
        ring.record("forensics module self-check");
        assert!(ring
            .snapshot()
            .iter()
            .any(|line| line.contains("self-check")));
    }
}
