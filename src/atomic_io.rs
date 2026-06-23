//! Small filesystem helper: atomic (write-temp-then-rename) file writes.
//!
//! `std::fs::write` truncates the target file and then streams bytes into it
//! in place; a crash, SIGKILL, or full disk mid-write can leave a torn or
//! truncated file behind. Writing the new content to a sibling temp file
//! first and `rename`-ing it over the target is atomic on the same
//! filesystem (POSIX `rename(2)` — the standard write-temp-then-rename
//! idiom), so a reader always sees either the complete old content or the
//! complete new content, never a partial write (QA round-1 H4 for
//! the document save path, M4 for `session.toml`).
//!
//! The naive version of this idiom (a fresh `File::create` + `rename`) is
//! itself a regression trap (QA round-2 N3/N4/N5, QA round-3 R3-1/R3-3, QA
//! round-4 R4-1, QA round-5 R5-1/R5-4), all fixed here:
//! - **N3**: `rename` replaces the target's *inode* — a fresh temp file
//!   defaults to the umask-derived mode, silently dropping a stricter
//!   `chmod`; and if `path` is a symlink, `rename` replaces the symlink
//!   itself with a plain file instead of writing through it. Fixed by
//!   canonicalizing `path` first (writing through any symlink to its real
//!   target) and copying the existing file's mode (and, best-effort, owner)
//!   onto the temp file before the rename.
//! - **R3-1** (the sharper form of N3 QA caught on re-review): copying the
//!   target's mode onto the temp file only AFTER `write_all`/`sync_all` still
//!   left a multi-millisecond window where a `chmod 600` file's private
//!   bytes sat in a world-readable (umask-derived, typically `0644`),
//!   predictably-named temp sibling any local user could read. Fixed by
//!   creating the temp file at a private mode (`0600`) from the very first
//!   byte — [`create_private_tmp_file`] — and only relaxing it to the
//!   target's real mode once the content is safely inside a private file.
//! - **N4**: a fixed temp-file name is a collision hazard (two instances
//!   writing the same file, or a pre-planted file at that exact name) —
//!   fixed with a process-id + nanosecond-timestamp suffix, created with
//!   `create_new` so a pre-existing file at that exact name is never
//!   silently reused (the same exclusivity discipline as `workaround.rs`'s
//!   `create_unique_secure_dir`, ScrAP-42).
//! - **N5**: `rename` is atomic but not otherwise guaranteed durable across
//!   a crash without an `fsync` on the containing directory — fixed by
//!   fsyncing the parent directory after the rename (best-effort: a failure
//!   here does not undo an already-correct write+rename, it just forgoes the
//!   extra crash-durability guarantee).
//! - **R3-3**: owner and mode restoration order matters — an unprivileged
//!   `chown(2)` to a uid/gid the file already has is a privilege no-op, but
//!   POSIX still has it clear any setuid/setgid bits. Doing it BEFORE the
//!   final `set_permissions` (which restores the target's real mode,
//!   including any such bits) means that restoration is never undone
//!   afterward. Both operations are fd-based (`fchown`/`fchmod` via the open
//!   `File`), not path-based, avoiding a second path resolution of
//!   `tmp_path` after creation.
//! - **R4-1**: R3-1's private (`0600`) creation mode is only ever relaxed
//!   back down when there's an EXISTING target mode to restore — a brand-new
//!   file (first save, or Save As to a new path) has none, so it silently
//!   stayed at `0600` (owner-only) instead of the user's normal umask
//!   default the way `std::fs::write` would have produced. Fail-safe
//!   direction (over-restrictive, not a disclosure) but surprising. Fixed by
//!   [`default_new_file_mode`] — the umask-derived `0666 & !umask` — applied
//!   whenever there is no existing mode to copy instead.
//! - **R5-1**: the umask query has no read-only getter — reading it means
//!   briefly SETTING it, which mutates process-GLOBAL state. Calling that on
//!   EVERY new-file save (R4-1's fix) reopens a race window each time;
//!   `cargo test`'s parallel execution reproduced it directly (two tests
//!   creating new files hit the query concurrently, corrupting the process
//!   umask for both). Fixed by querying it exactly ONCE per process, cached
//!   in a `OnceLock` ([`default_new_file_mode`]) — every call after the first
//!   is a plain cached read, no further syscalls or global mutation.
//! - **R5-4**: relaxing the mode used to fold every `stat` outcome other than
//!   success into "brand-new file" (`Option::None` from `.ok()`), including a
//!   stat failure for a reason OTHER than "doesn't exist" (permissions, a
//!   transient error) — which could WIDEN an existing restrictive file to the
//!   umask-derived public default instead of leaving it alone. Fixed by
//!   [`TargetMode`], which distinguishes "doesn't exist" from "unknown" and
//!   leaves the temp at its private creation mode in the unknown case
//!   (over-restrictive, the same safe direction as every fix above).

use std::io::Write;
use std::path::{Path, PathBuf};

/// What is known about the save target immediately before writing: an
/// existing file (with its real metadata), definitely absent (`NotFound` —
/// safe to treat as a brand-new file), or unknown (some other `stat` failure
/// — permissions, a transient error — where the target might still exist
/// with a restrictive mode we simply couldn't read). QA round-5 R5-4:
/// collapsing the latter two into one `Option::None` risked WIDENING an
/// existing restrictive file's mode; keeping them distinct lets
/// [`relax_mode_best_effort`] treat "unknown" as "leave it private" rather
/// than guessing.
enum TargetMode {
    Existing(std::fs::Metadata),
    New,
    Unknown,
}

fn probe_target_mode(target: &Path) -> TargetMode {
    match std::fs::metadata(target) {
        Ok(m) => TargetMode::Existing(m),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => TargetMode::New,
        Err(_) => TargetMode::Unknown,
    }
}

/// Deletes the temp file it names when dropped, unless [`disarm`](Self::disarm)
/// is called first.
///
/// **Why a guard and not a `remove_file` on each error path.** The cleanup used to
/// sit on exactly one path — a failed rename — while `write_all` and `sync_all`
/// returned through `?` and left the temp file behind. That is the common failure,
/// not the rare one: a full disk or a quota fails the WRITE, and every such save
/// dropped a `.scribtmp` sibling next to the user's document, with the name
/// deliberately randomised so they accumulate rather than overwrite. A guard makes
/// cleanup the default and success the exception, so a `?` added here later cannot
/// reintroduce the leak by omission.
struct TempFileGuard {
    /// `None` once disarmed — the rename succeeded and the file is now the target.
    path: Option<PathBuf>,
}

impl TempFileGuard {
    fn disarm(&mut self) {
        self.path = None;
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            // Best-effort by necessity: this runs while unwinding an error that is
            // about to be reported, and a failure to clean up must not mask it.
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Write `content` to `path` atomically (write-temp-then-rename). See the
/// module doc for the mode/owner-preservation, symlink-following, unique
/// temp-name, and parent-fsync behavior.
pub(crate) fn write_atomic(path: &Path, content: &str) -> std::io::Result<()> {
    // Write through an existing symlink to its real target rather than
    // replacing the symlink with a plain file (N3). A target that doesn't
    // exist yet (first save of a new document) has nothing to canonicalize —
    // `path` itself is already the right place to write.
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let target_mode = probe_target_mode(&target);

    let (tmp_path, mut f) = create_unique_private_tmp_file(&target)?;
    // Armed immediately: from here every early return, including each `?` below,
    // must take the temp file with it.
    let mut cleanup = TempFileGuard {
        path: Some(tmp_path.clone()),
    };
    {
        f.write_all(content.as_bytes())?;
        // R3-3: owner before mode (see module doc) — both fd-based.
        preserve_owner_best_effort(&f, &target_mode);
        // Relax from the private creation mode to the real target mode
        // (R3-1) now that the content is safely inside a private file.
        relax_mode_best_effort(&f, &target_mode);
        f.sync_all()?;
    }
    // Close before renaming. On unix this is immaterial, but Windows refuses to
    // rename a file that is still open without FILE_SHARE_DELETE, which `File` does
    // not request — so leaving it open would fail every save on the platform that
    // cannot be tested from here.
    drop(f);
    let rename_result = std::fs::rename(&tmp_path, &target);
    if rename_result.is_ok() {
        // The temp file IS the target now; removing it would delete the save.
        cleanup.disarm();
        fsync_parent_dir(&target); // N5, best-effort
    }
    rename_result
}

/// Create a fresh, private ([`create_private_tmp_file`], R3-1) temp file next
/// to `target`, retrying with a freshly generated unique name (QA round-4
/// R4-3) on the astronomically unlikely chance that [`unique_tmp_path_for`]'s
/// name collides with an existing file. Previously a single collision
/// surfaced as a hard save error via `?` instead of simply trying again.
fn create_unique_private_tmp_file(target: &Path) -> std::io::Result<(PathBuf, std::fs::File)> {
    const MAX_ATTEMPTS: u32 = 5;
    let mut last_err = None;
    for _ in 0..MAX_ATTEMPTS {
        let tmp_path = unique_tmp_path_for(target);
        match create_private_tmp_file(&tmp_path) {
            Ok(f) => return Ok((tmp_path, f)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => last_err = Some(e),
            Err(e) => return Err(e),
        }
    }
    Err(last_err.unwrap_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "temp file name collision",
        )
    }))
}

/// Create `tmp_path` exclusively (fails if it already exists — N4) and
/// PRIVATE from the very first byte (mode `0600` on unix — R3-1): the file
/// must never be briefly world-readable while content that may be as
/// sensitive as the final target's restrictive mode implies is being written
/// into it, before [`write_atomic`] relaxes the mode to match the target's
/// real one after the write completes.
#[cfg(unix)]
fn create_private_tmp_file(tmp_path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(tmp_path)
}

#[cfg(not(unix))]
fn create_private_tmp_file(tmp_path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(tmp_path)
}

/// A temp path alongside `path` that is unique per call (N4): a process id +
/// nanosecond-timestamp + monotonic-counter suffix, opened with `create_new`
/// by the caller so a pre-existing file at that exact name is never silently
/// reused. The counter (rather than the timestamp alone) guarantees a
/// distinct name across rapid retries within one process even on a clock
/// whose resolution doesn't distinguish two nanosecond reads back-to-back
/// (`create_unique_private_tmp_file`'s retry loop, R4-3).
fn unique_tmp_path_for(path: &Path) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.{nanos}.{n}.scribtmp", std::process::id()));
    path.with_file_name(name)
}

/// Relax `f` from its private creation mode (`0600`, R3-1) to the mode it
/// should actually end up at: the existing target's own mode when there is
/// one, the umask-derived default for a brand-new file (R4-1), or — R5-4 —
/// left alone (private) when the target's status is unknown, since it might
/// still exist with a restrictive mode a `stat` failure just couldn't read.
#[cfg(unix)]
fn relax_mode_best_effort(f: &std::fs::File, target_mode: &TargetMode) {
    use std::os::unix::fs::PermissionsExt;
    let mode = match target_mode {
        TargetMode::Existing(existing) => existing.permissions().mode(),
        TargetMode::New => default_new_file_mode(),
        TargetMode::Unknown => return,
    };
    if let Err(e) = f.set_permissions(std::fs::Permissions::from_mode(mode)) {
        // QA round-4 R4-2: on the rare failure, `let _ =` used to leave an
        // EXISTING file silently narrowed to the private 0600 creation mode
        // with no trace for a future bug report to grep.
        log::warn!("atomic_io: could not restore mode {mode:o} after save: {e}");
    }
}

#[cfg(not(unix))]
fn relax_mode_best_effort(f: &std::fs::File, target_mode: &TargetMode) {
    if let TargetMode::Existing(existing) = target_mode {
        // Logged rather than discarded, for the same reason as the unix arm above —
        // but a MUCH smaller hazard, and worth stating so the two are not read as
        // equivalent: `std::fs::Permissions` carries only the read-only bit on
        // Windows, so the failure here is "the read-only flag was not restored",
        // never the unix arm's silent narrowing to 0600.
        if let Err(e) = f.set_permissions(existing.permissions()) {
            log::warn!("atomic_io: could not restore the read-only flag after save: {e}");
        }
    }
}

/// The mode a brand-new file should get: the POSIX default (`0666`, i.e. no
/// execute bit) masked by the process's current umask — matching what
/// `std::fs::write`/`File::create` would have produced by letting the OS
/// apply the umask at creation time. We can't rely on that here, because the
/// temp file is deliberately created at an explicit `0600` for the write
/// window (R3-1), so once there is no existing target mode to restore
/// instead (R4-1) we must apply the umask ourselves.
///
/// Cached in a `OnceLock` (QA round-5 R5-1): [`query_umask_via_round_trip`]
/// is the only way to read `umask(2)` (there is no read-only getter — it
/// must be briefly SET to read the old value back, then restored), which
/// mutates process-GLOBAL state. Doing that on every call reopens a race
/// window each time; reading it exactly once and caching the result removes
/// the recurring mutation entirely — every call after the first is a plain
/// read with no syscall at all.
#[cfg(unix)]
fn default_new_file_mode() -> u32 {
    static CACHED_UMASK: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    let mask = *CACHED_UMASK.get_or_init(query_umask_via_round_trip);
    0o666 & !mask
}

/// Read the process's current umask via the standard "set it, read the old
/// value back, restore it" round-trip — POSIX has no read-only getter.
/// Exposed as its own function (rather than inlined into
/// [`default_new_file_mode`]) so it can be tested in isolation under a
/// controlled umask; call sites needing a new file's mode should go through
/// the cached [`default_new_file_mode`] instead, not this directly.
#[cfg(unix)]
fn query_umask_via_round_trip() -> u32 {
    // SAFETY: `umask(2)` has no documented failure mode. The real hazard
    // here isn't safety but the process-global mutation between the two
    // calls (see both this function's and `default_new_file_mode`'s docs).
    unsafe {
        let old = libc::umask(0o022);
        libc::umask(old);
        old as u32
    }
}

#[cfg(unix)]
fn preserve_owner_best_effort(f: &std::fs::File, target_mode: &TargetMode) {
    use std::os::unix::fs::MetadataExt;
    // Best-effort: an unprivileged process can only "change" ownership to
    // the uid/gid it already has, but that IS the common case here —
    // preserving a file the running user already owns, e.g. after a
    // privileged install step chowned it. Ignored on failure (mode is the
    // primary guarantee; owner preservation is a bonus, not a promise) —
    // notably including EPERM, the expected outcome for an unprivileged
    // process attempting to chown to any OTHER uid/gid.
    if let TargetMode::Existing(existing) = target_mode {
        let _ = std::os::unix::fs::fchown(f, Some(existing.uid()), Some(existing.gid()));
    }
}

#[cfg(not(unix))]
fn preserve_owner_best_effort(_f: &std::fs::File, _target_mode: &TargetMode) {}

/// Best-effort: fsync the parent directory so the rename above is durable
/// across a crash, not just atomic (N5). A failure here (e.g. an unsupported
/// platform) does not undo an already-successful write+rename.
fn fsync_parent_dir(path: &Path) {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        if let Ok(d) = std::fs::File::open(dir) {
            let _ = d.sync_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No leftover `.scribtmp`-suffixed file in `dir` after a call.
    fn no_temp_files_left(dir: &Path) -> bool {
        std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .all(|e| !e.file_name().to_string_lossy().contains(".scribtmp"))
    }

    /// The guard is what makes every `write_atomic` failure path clean up, and the
    /// paths that matter — a failed `write_all` on a full disk, a failed `sync_all` —
    /// cannot be provoked from a unit test without a filesystem that can be filled.
    /// So the MECHANISM is tested directly instead: an armed guard deletes on drop.
    /// Without this, the only coverage of the leak fix would be the rename path that
    /// was already covered before it, which is the "guard test exercises the
    /// already-closed half" shape this review flagged elsewhere.
    #[test]
    fn armed_temp_file_guard_deletes_its_file_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let tmp = dir.path().join("doc.md.1234.scribtmp");
        std::fs::write(&tmp, "half-written").unwrap();
        {
            let _guard = TempFileGuard {
                path: Some(tmp.clone()),
            };
        }
        assert!(!tmp.exists(), "armed guard must remove the temp file");
        assert!(no_temp_files_left(dir.path()));
    }

    /// The converse, and the one that would corrupt a save if it regressed: a
    /// disarmed guard must NOT delete, because after a successful rename that path
    /// names the user's document, not a temp file.
    #[test]
    fn disarmed_temp_file_guard_leaves_its_file_alone() {
        let dir = tempfile::tempdir().unwrap();
        let saved = dir.path().join("doc.md");
        std::fs::write(&saved, "the document").unwrap();
        {
            let mut guard = TempFileGuard {
                path: Some(saved.clone()),
            };
            guard.disarm();
        }
        assert_eq!(std::fs::read_to_string(&saved).unwrap(), "the document");
    }

    /// Proves the guard is actually WIRED INTO `write_atomic`, not merely correct in
    /// isolation — so the temp file must genuinely be created and then orphaned.
    ///
    /// A directory standing where the target file should be does exactly that: the
    /// parent is writable so the temp file is created and written normally, and the
    /// rename onto a directory then fails (`EISDIR`/`ENOTDIR`). A read-only parent
    /// was tried first and is NOT equivalent — it fails at `create_new`, so no temp
    /// file ever exists and the test passes with the cleanup deleted. It was vacuous
    /// in exactly the way the review flagged elsewhere; mutation testing caught it.
    #[test]
    fn write_atomic_leaves_no_temp_file_when_the_rename_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        std::fs::create_dir(&path).unwrap();

        let result = write_atomic(&path, "new");

        assert!(
            result.is_err(),
            "renaming onto a directory must fail the save"
        );
        assert!(
            no_temp_files_left(dir.path()),
            "the orphaned temp file must be cleaned up"
        );
    }

    #[test]
    fn write_atomic_creates_the_target_with_the_given_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        write_atomic(&path, "hello").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
        assert!(no_temp_files_left(dir.path()));
    }

    #[test]
    fn write_atomic_overwrites_existing_content_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        std::fs::write(&path, "old").unwrap();
        write_atomic(&path, "new").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        assert!(no_temp_files_left(dir.path()));
    }

    #[cfg(unix)]
    #[test]
    fn write_atomic_new_file_is_not_locked_to_the_private_creation_mode() {
        // QA round-4 R4-1: a brand-new file must not stay at write_atomic's
        // private 0600 creation mode. Doesn't assert an exact umask-derived
        // value: `default_new_file_mode`'s cache (R5-1) is process-global
        // and shared with every other test in this binary, so which umask
        // ends up cached depends on test execution order — see
        // `query_umask_via_round_trip`'s own test below for the umask logic
        // itself, tested in isolation under a controlled value.
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.md");

        write_atomic(&path, "brand new document").unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_ne!(
            mode, 0o600,
            "a brand-new file must not stay at the private creation mode"
        );
    }

    // `umask(2)` is process-global, so any test that reads/sets it must be
    // serialized against every other such test (parallel test threads would
    // otherwise race on the same process-wide value) — the same discipline
    // `session.rs` already uses for its `XDG_STATE_HOME` env-var tests.
    #[cfg(unix)]
    static UMASK_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[cfg(unix)]
    #[test]
    fn query_umask_via_round_trip_reads_and_restores_the_process_umask() {
        // QA round-5 R5-1/R5-2: tests the round-trip primitive directly
        // (bypassing `default_new_file_mode`'s cache, which — correctly for
        // production, R5-1 — makes the SYSCALL only once per process and so
        // can't be probed repeatably from a single test). Uses a distinctive
        // value (0o077) rather than the round-trip's own internal probe
        // constant (0o022, R5-2) so a broken restore would actually be
        // caught instead of coincidentally matching.
        let _guard = UMASK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // QA round-6 R6-1: prime `default_new_file_mode`'s cache — under
        // THIS lock, before touching the live umask below — so its one-time
        // init (which performs the very same set/read/restore round-trip)
        // can never fire concurrently with this test's own direct probe. If
        // some other, unlocked new-file test already raced ahead and primed
        // it first, this is a pure cached read (no syscall, nothing to
        // race); either way, no cache-init round-trip can happen AFTER this
        // point, which is what would otherwise be free to interleave with
        // (and corrupt, or be corrupted by) the umask(0o077) probe below.
        let _ = default_new_file_mode();
        let original = unsafe { libc::umask(0o077) };

        let observed = query_umask_via_round_trip();

        let restored = unsafe { libc::umask(original) };
        assert_eq!(
            observed, 0o077,
            "must read back exactly the umask that was set before the call"
        );
        assert_eq!(
            restored, 0o077,
            "must restore the umask it read, not leave the process at its own probe value"
        );
    }

    #[cfg(unix)]
    #[test]
    fn relax_mode_best_effort_leaves_the_private_mode_alone_when_target_status_is_unknown() {
        // QA round-6 R6-2: the only meaningfully untested branch from R5-4 —
        // a `stat` failure that ISN'T "not found" must leave the temp file
        // at its private creation mode rather than guessing (the target
        // might still exist with a restrictive mode the failed `stat`
        // simply couldn't read; over-restrictive is the safe direction).
        // Tests the decision directly via a constructed `TargetMode::Unknown`
        // rather than trying to induce a real non-`NotFound` `stat` failure —
        // a permission-based approach (stripping a parent directory's search
        // bit) is unreliable in any environment that runs tests as root,
        // where permission checks are bypassed entirely.
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let tmp_path = dir.path().join("doc.md.999.999.scribtmp");
        let f = create_private_tmp_file(&tmp_path).unwrap();

        relax_mode_best_effort(&f, &TargetMode::Unknown);

        let mode = f.metadata().unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "an unknown target status must leave the temp file at its private \
             creation mode, never widen it"
        );
    }

    #[cfg(unix)]
    #[test]
    fn write_atomic_preserves_the_target_s_existing_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.md");
        std::fs::write(&path, "old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

        write_atomic(&path, "new").unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "save must not widen an existing restrictive mode"
        );
    }

    #[cfg(unix)]
    #[test]
    fn create_private_tmp_file_is_never_world_readable_at_creation() {
        // QA round-3 R3-1: the temp file must be private from the INSTANT it
        // is created — before any content is written and before its mode is
        // ever relaxed to match a looser target mode — so a `chmod 600`
        // file's secret content is never briefly readable through a
        // world-readable temp sibling. Tested at the creation primitive
        // directly, since `write_atomic` itself is synchronous and leaves no
        // window in which a caller could observe the temp file mid-write.
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let tmp_path = dir.path().join("doc.md.123.456.scribtmp");

        let f = create_private_tmp_file(&tmp_path).unwrap();

        let mode = f.metadata().unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "the temp file must be private (owner rw only) at the moment it is created"
        );
    }

    /// Saving a document reached through a symlink must write THROUGH the link to its
    /// real target, not replace the link with a plain file — `rename(2)` replaces the
    /// link itself, so a write-temp-then-rename defeats a file symlink by design
    /// unless the path is canonicalized first.
    ///
    /// Runtime skip rather than `#[cfg(unix)]` (ScrAP-212): Windows has symlinks too,
    /// and this tree ships there. Under the exclusion the guard did not exist on
    /// Windows — not skipped, not counted — so nothing on that platform ever checked
    /// that a save through a link keeps the link.
    #[test]
    fn write_atomic_writes_through_a_symlink_instead_of_replacing_it() {
        use crate::testsymlink::symlink_or_skip;
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.md");
        let link = dir.path().join("link.md");
        std::fs::write(&real, "old").unwrap();
        if symlink_or_skip(&real, &link, "save through a symlink").is_err() {
            return;
        }

        write_atomic(&link, "new").unwrap();

        assert!(
            std::fs::symlink_metadata(&link).unwrap().is_symlink(),
            "the symlink itself must survive the save"
        );
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(&link).unwrap(), "new");
    }

    /// Longest path the legacy (non-verbatim) Win32 API accepts, including
    /// its terminating NUL. Going beyond it needs either the `\\?\` prefix —
    /// which Rust's std applies internally for absolute paths — or the
    /// system-wide `LongPathsEnabled` setting.
    #[cfg(windows)]
    const MAX_PATH: usize = 260;

    #[cfg(windows)]
    #[test]
    fn write_atomic_survives_a_document_directory_that_overflows_max_path() {
        // `unique_tmp_path_for` puts the temp file NEXT TO the target — it
        // must, for the rename to stay atomic on one volume — so the overflow
        // is driven by the DOCUMENT's directory, not by a deep temp directory.
        // A document path that is itself perfectly legal can therefore produce
        // a temp sibling that is not, which makes the save path the only place
        // the limit bites and a deep workspace no substitute for this test.
        //
        // Both preconditions are asserted, not assumed, and both are derived
        // from the real `unique_tmp_path_for`: a change to its suffix can then
        // never leave this test silently exercising nothing.
        //
        // The outcome is deliberately not pinned to success. With
        // `LongPathsEnabled=1` the write is expected to complete; with it
        // disabled the constrained case runs instead. What must hold either
        // way is the atomicity contract: the save completes or fails cleanly,
        // leaves no temp litter, and never destroys existing content.
        const COMPONENT: &str = "a-deliberately-long-directory-component";
        const DOC_FILE_NAME: &str = "doc.md";
        // A padding component costs a separator plus at least one character.
        const MIN_COMPONENT_COST: usize = 2;

        let root = tempfile::tempdir().unwrap();
        let mut dir = root.path().to_path_buf();
        while dir.as_os_str().len() + 1 + COMPONENT.len() + 1 + DOC_FILE_NAME.len() < MAX_PATH {
            dir.push(COMPONENT);
        }
        // Spend the slack the loop above leaves, so the document path lands as
        // close under the limit as it can. The loop alone can stop a whole
        // component short, and on a host with a shorter temp root that is
        // enough to leave the temp sibling under the limit as well — which the
        // precondition below would then (correctly) report as a test that no
        // longer covers anything. Padding keeps it covering the case instead.
        let used = dir.as_os_str().len() + 1 + DOC_FILE_NAME.len();
        if let Some(pad) = (MAX_PATH - 1)
            .checked_sub(used)
            .filter(|slack| *slack >= MIN_COMPONENT_COST)
        {
            dir.push("p".repeat(pad - 1));
        }
        let doc = dir.join(DOC_FILE_NAME);

        assert!(
            doc.as_os_str().len() < MAX_PATH,
            "precondition: the document path must itself be legal, got {} chars",
            doc.as_os_str().len()
        );
        assert!(
            unique_tmp_path_for(&doc).as_os_str().len() > MAX_PATH,
            "precondition: the temp sibling must overflow MAX_PATH, or this test \
             no longer exercises the case it was written for"
        );

        std::fs::create_dir_all(&dir).unwrap();

        // Brand-new file: nothing to canonicalize, so the un-prefixed path is
        // what reaches the filesystem — the longest form `write_atomic` sees.
        let created = write_atomic(&doc, "first");
        match &created {
            Ok(()) => assert_eq!(std::fs::read_to_string(&doc).unwrap(), "first"),
            Err(_) => assert!(
                !doc.exists(),
                "a save that could not complete must not leave a partial document"
            ),
        }
        assert!(
            no_temp_files_left(&dir),
            "no temp litter after a new-file save"
        );

        // Overwrite: the target now exists, so it canonicalizes to a `\\?\`
        // form that is longer again. A failure here must leave the previous
        // content intact — that is the guarantee the whole module exists for,
        // and the one a path-length failure could plausibly break.
        if created.is_ok() {
            let overwritten = write_atomic(&doc, "second");
            let expected = if overwritten.is_ok() {
                "second"
            } else {
                "first"
            };
            assert_eq!(std::fs::read_to_string(&doc).unwrap(), expected);
            assert!(
                no_temp_files_left(&dir),
                "no temp litter after an overwrite"
            );
        }
    }

    #[test]
    fn write_atomic_two_calls_do_not_collide_on_temp_names() {
        // N4: distinct calls must not race on the same temp path even when
        // issued back-to-back (previously a fixed ".scribtmp" name).
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.md");
        let b = dir.path().join("b.md");
        write_atomic(&a, "a-content").unwrap();
        write_atomic(&b, "b-content").unwrap();
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "a-content");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "b-content");
        assert!(no_temp_files_left(dir.path()));
    }
}
