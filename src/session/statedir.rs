//! Where this application's own state lives on disk, and how that directory comes
//! into existence.
//!
//! Two jobs, and they are here together because the second is only correct if it is
//! the ONLY way the first's directory is ever created: the tree it resolves holds
//! the paths of every open document and every crash report, so the directory's MODE
//! is a privacy boundary rather than a detail. Three call sites used to create it
//! with a bare `create_dir_all` and landed it group- and world-readable.
//!
//! Consumed by [`super::load`]/[`super::save`] and, for the same tree, by
//! `crate::forensics` — one lookup, so the two cannot drift into different places.

use std::path::{Path, PathBuf};

/// Where the session file lives, or `None` if no state directory can be located
/// (in which case save and load both no-op and the app simply starts fresh).
pub(super) fn session_path() -> Option<PathBuf> {
    Some(state_directory()?.join("session.toml"))
}

/// The application's state directory (`…/scribobulate`, or
/// `…/scribobulate/instances/<name>` for a named instance), or `None` if none can be
/// located.
///
/// **The single state-directory lookup in the tree.** `session.toml` and the
/// crash-forensics log and reports (`forensics::state_directory`) both resolve
/// through here, so they cannot drift into different places, and the warning below
/// is emitted once for the process rather than once per consumer.
///
/// `XDG_STATE_HOME` is checked FIRST on **every** platform, and read **live** rather
/// than cached — [`with_state_home_for_test`] sets it at runtime to redirect the
/// tests at a temp dir, and 14 call sites across this module and `window/mod.rs`
/// depend on that still working. This is also why the lookup is hand-rolled from
/// `std::env` rather than delegating to `glib::user_state_dir()`: GLib caches its
/// answer on first call and would ignore the helper entirely.
///
/// Only the FALLBACK differs per platform, because `HOME` is a POSIX convention that
/// Windows does not set — assuming it there left this returning `None` forever, so
/// nothing persisted at all (GEP-42).
pub(crate) fn state_directory() -> Option<PathBuf> {
    let Some(base) = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(state_home_fallback)
    else {
        // Warn once, not per save/load. This hid for a whole port precisely
        // because the failure was silent — "nothing restored" is indistinguishable
        // from "nothing was ever saved" (GEP-42).
        //
        // Gated on the warning being DELIVERABLE, not just on having warned (QA round
        // 5, L-4). The first caller is `forensics::install`, which runs at
        // `logging.rs:131` — one line BEFORE `log::set_logger` at :132. With no logger
        // installed, `log::warn!` is a no-op against a max-level of `Off`, so that call
        // dropped the message into a void *and consumed the latch*, and every later
        // caller — the ones that run with a working logger — stayed silent forever.
        // A one-shot warning that fires exactly once, before anything can hear it, is
        // the GEP-42 silence rebuilt inside its own fix.
        if log::log_enabled!(log::Level::Warn) {
            static WARNED: std::sync::Once = std::sync::Once::new();
            WARNED.call_once(|| {
                log::warn!(
                    "no user state directory could be located \
                 (checked XDG_STATE_HOME, then the platform fallback); \
                 window geometry and open tabs will not persist"
                );
            });
        }
        return None;
    };
    // A named instance keeps its state one level down, in `instances/<name>`, so the
    // default instance's folder is the one it always used (TDD 8.12, 8.15). Applied
    // here, in the single lookup, so session, swap and forensics all follow it.
    Some(crate::instance::state_dir_for(
        base.join("scribobulate"),
        crate::instance::current(),
    ))
}

/// Create the state directory **private to the owning user**, and tighten it if an
/// earlier version left it open.
///
/// **The one place the state directory comes into existence.** Three call sites created
/// it with a bare `create_dir_all` — `session::save`, `forensics::sink`,
/// `forensics::report` — which is `0777 & !umask`, so under the common `umask 0002` it
/// landed at **0775**. MEASURED on the reference machine before this existed:
///
/// ```text
/// drwxrwxr-x  ~/.local/state/scribobulate
/// -rw-rw-r--  session.toml
/// ```
///
/// Group- and world-readable, and `session.toml` records **the paths of every open
/// document** along with window geometry. The crash reports beside it were correctly
/// `0600` via [`crate::forensics::private_options`], but a private file in a traversable
/// directory still leaks its *name*, and these names carry a timestamp and a pid.
///
/// **Why the directory and not each file.** `session.toml` is written through
/// `atomic_io::write_atomic`, which deliberately *relaxes* a new file from its private
/// creation mode to the umask default — correct, because the same function saves the
/// user's own documents, where forcing `0600` would be wrong. So there is no per-file fix
/// that does not either break document saving or get re-omitted at the next call site.
/// A `0700` directory subsumes all of it: nothing inside is reachable by another user
/// whatever its own mode says.
///
/// That is also the **platform-symmetric** answer. The Windows seat measured the identical
/// exposure there and reached the same place from the other direction: `OpenOptionsExt`
/// has no security-descriptor hook, so file-level privacy is not expressible at all, and
/// the fix is a PROTECTED DACL on the directory that everything inside then inherits.
/// POSIX inheritance is traversal rather than ACEs, but the seam is the same one, which is
/// what lets TDD 21.12 be stated once for both platforms instead of per-platform.
/// `0700` is also what the XDG Base Directory spec asks for.
///
/// Existing directories are tightened, not just new ones — otherwise every installation
/// that has already run keeps the open mode forever, which is the same
/// only-applies-on-creation trap that made the signal handler's `0600` a no-op
/// (GTK4Rs/AP-108/GTK4Rs/AP-130). Only this app's own leaf directory is touched, never a parent.
pub(crate) fn create_state_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        // Whether this is a MIGRATION or a fresh creation, decided before creating.
        //
        // An unconditional tighten would be simpler and equally safe — and that is what
        // this did first. It was wrong in a way worth recording: it made the creation
        // mode dead weight, because the chmod that followed corrected 0755 just as
        // happily as it corrected 0775. MUTATION-TESTED: changing `.mode(0o700)` to
        // `.mode(0o755)` left the guard GREEN. Two mechanisms where one carries the
        // property is a mechanism nothing tests, and this round is largely about those.
        //
        // Splitting them makes each load-bearing: creation privacy is what leaves no
        // window in which the directory exists world-readable (a chmod-after-create has
        // one), and the tighten is what reaches installations that already ran. Racing
        // this only loses if another process creates the directory between the check and
        // the create — and the only thing that does is another instance of this app,
        // which creates it 0700.
        let migrating = std::fs::symlink_metadata(dir).is_ok();
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
        if migrating {
            // `create_dir_all` leaves an EXISTING directory's mode alone, so a tree made
            // by an earlier build stays 0775 until something narrows it.
            //
            // **A tighten that does not take is an error, as on Windows.** An existing
            // directory has no `0700`-created floor under it: if the chmod is refused
            // (owned by someone else, a read-only mount) the directory stays readable by
            // other local users, and callers writing unsaved document text into it must
            // be able to decline. The session save already degrades on an `Err`.
            let meta = std::fs::metadata(dir)?;
            if meta.permissions().mode() & 0o077 != 0 {
                if let Err(e) =
                    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
                {
                    log::warn!(
                        "state dir {}: could not narrow mode {:o} to 0700: {e}",
                        dir.display(),
                        meta.permissions().mode() & 0o777
                    );
                    return Err(e);
                }
            }
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        // Windows needs a PROTECTED DACL, which `std::fs` cannot express — hence the
        // seam. `create_private_directory` creates with `SECURITY_ATTRIBUTES` so there
        // is no instant in which the directory exists unprotected, and tightens an
        // existing one via `SetNamedSecurityInfoW`, mirroring the create/migrate split
        // above for the same reason: a creation-time-only fix reaches nobody who has
        // already run the app.
        //
        // The error is PROPAGATED, not swallowed. Unlike the unix tighten there is no
        // `0700`-created floor underneath a failure here: if the DACL does not land the
        // directory may be genuinely writable by other local users, and callers writing
        // unsaved document text into it need to be able to decline. See
        // `platform/win32/privacy.rs` for the measured exposure this closes.
        crate::platform::win32::create_private_directory(dir)
    }
    #[cfg(all(not(unix), not(windows)))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// The platform fallback, read from the process environment.
fn state_home_fallback() -> Option<PathBuf> {
    state_home_fallback_in(|var| std::env::var_os(var))
}

/// POSIX fallback: `~/.local/state`, per the XDG Base Directory spec.
///
/// Takes the environment as a lookup so a test can assert the convention —
/// `.cargo/config.toml` pins `XDG_STATE_HOME`, so no other test reaches this.
#[cfg(unix)]
fn state_home_fallback_in(env: impl Fn(&str) -> Option<std::ffi::OsString>) -> Option<PathBuf> {
    env("HOME").map(|h| PathBuf::from(h).join(".local").join("state"))
}

/// Windows fallback: **Local** AppData, deliberately not Roaming. Session state is
/// window geometry, open tabs and per-monitor layout — machine-specific things that
/// should not follow a roaming profile onto a different machine with a different
/// screen setup. User *configuration* takes the opposite decision; see
/// `config::config_home_fallback_in`.
#[cfg(windows)]
fn state_home_fallback_in(env: impl Fn(&str) -> Option<std::ffi::OsString>) -> Option<PathBuf> {
    env("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| env("USERPROFILE").map(|p| PathBuf::from(p).join("AppData").join("Local")))
}

#[cfg(test)]
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Run `f` with `XDG_STATE_HOME` pointed at `dir` for the duration, restoring
/// the prior value on the way out. Holds [`ENV_LOCK`] for the duration.
///
/// **Every window `f` opened is closed first, with each of its tabs' snapshots
/// discarded**, before the redirect is lifted, and on a failing test as well as a
/// passing one. A dirty tab left alive keeps a snapshot timer armed, and when it fires
/// it resolves the swap directory from `XDG_STATE_HOME` at that moment, which is
/// process-global: a later test's redirect then receives the stray write and its
/// recovery reopens it into a tab that test never asked for. Windows that existed
/// before `f` are left alone.
#[cfg(test)]
pub(crate) fn with_state_home_for_test<T>(dir: &std::path::Path, f: impl FnOnce() -> T) -> T {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _restore = RestoreStateHome {
        prev: std::env::var_os("XDG_STATE_HOME"),
        windows_before: crate::window::toplevels_for_test(),
    };
    std::env::set_var("XDG_STATE_HOME", dir);
    f()
}

/// The teardown half of [`with_state_home_for_test`], in a `Drop` so a test that fails
/// is cleaned up too. Declared after the lock guard, so it runs while the lock is held.
#[cfg(test)]
struct RestoreStateHome {
    prev: Option<std::ffi::OsString>,
    windows_before: Vec<gtk::Window>,
}

#[cfg(test)]
impl Drop for RestoreStateHome {
    fn drop(&mut self) {
        crate::window::close_windows_opened_since_for_test(&self.windows_before);
        match self.prev.take() {
            Some(v) => std::env::set_var("XDG_STATE_HOME", v),
            None => std::env::remove_var("XDG_STATE_HOME"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fallback follows the host's convention, through a fake environment — the
    /// real one has `XDG_STATE_HOME` pinned, so nothing else reaches it. Each platform
    /// asserts its own convention.
    #[test]
    fn the_state_fallback_follows_the_platform_convention() {
        use std::ffi::OsString;
        use std::path::PathBuf;
        let env = |vars: &'static [(&'static str, &'static str)]| {
            move |var: &str| {
                vars.iter()
                    .find(|(k, _)| *k == var)
                    .map(|(_, v)| OsString::from(v))
            }
        };
        let fallback = |vars| super::state_home_fallback_in(env(vars));

        assert_eq!(fallback(&[]), None, "no base variable, no directory");
        if cfg!(windows) {
            assert_eq!(
                fallback(&[("LOCALAPPDATA", "local"), ("USERPROFILE", "profile")]),
                Some(PathBuf::from("local")),
                "LOCALAPPDATA wins"
            );
            assert_eq!(
                fallback(&[("USERPROFILE", "profile")]),
                Some(PathBuf::from("profile").join("AppData").join("Local")),
                "without LOCALAPPDATA, the profile's Local directory"
            );
        } else {
            assert_eq!(
                fallback(&[("HOME", "home")]),
                Some(PathBuf::from("home").join(".local").join("state"))
            );
        }
    }

    /// The state directory is private to its owner, and an over-permissive one is
    /// tightened (QA round 5, M-6's Linux half).
    ///
    /// MEASURED before the fix, on the reference machine under the default `umask 0002`:
    /// `drwxrwxr-x ~/.local/state/scribobulate` holding `-rw-rw-r-- session.toml` — the
    /// paths of every open document, world-readable. The crash reports beside it were
    /// already `0600`, which is why this went unnoticed: the per-file seam was correct
    /// and the directory containing it was not, so a private file sat in a traversable
    /// directory advertising its own name.
    ///
    /// Both halves are asserted because each is now load-bearing, and getting that true
    /// took a second attempt worth recording: the first version tightened
    /// unconditionally, which made the *creation* mode untested dead weight —
    /// MUTATION-TESTED, `.mode(0o700)` → `.mode(0o755)` stayed GREEN, because the chmod
    /// that followed corrected either one. `create_state_dir` now tightens only when
    /// migrating, so this kills both mutants: 0755-at-creation fails the fresh case, and
    /// dropping the tighten fails the stale case. The stale case is the
    /// only-applies-on-creation trap that made the signal handler's `0600` a no-op.
    ///
    /// **Both platforms now assert, by their own privacy mechanism.** Windows carries the
    /// property on a PROTECTED DACL rather than on mode bits, so the Windows arm reads
    /// the DACL back out of the OS and checks the same two things the unix arm does: that
    /// a freshly created directory is private, and that a directory an earlier build left
    /// open is TIGHTENED. Asserting the SDDL we passed in would prove nothing — this
    /// reads what the OS stored.
    #[test]
    fn the_state_directory_is_private_to_its_owner() {
        #[cfg(windows)]
        {
            use super::create_state_dir;

            // `P` in the DACL flags is the protected bit — inheritance severed. Without
            // it our ACEs would sit alongside the inherited permissive ones and the
            // directory would still be exposed, which is the exact defect this closes.
            // `AU` (Authenticated Users) and `BU` (BUILTIN\Users) are the two the
            // off-profile volume was granting; on a second volume they arrived as
            // `(I)(M)` — MODIFY, not merely read.
            let assert_private = |dir: &std::path::Path, case: &str| {
                let sddl = crate::platform::win32::directory_dacl_sddl(dir)
                    .unwrap_or_else(|| panic!("{case}: could not read the DACL back"));
                assert!(
                    sddl.starts_with("D:P"),
                    "{case}: DACL is not PROTECTED, so it still inherits: {sddl}",
                );
                for principal in [";AU)", ";BU)", ";WD)"] {
                    assert!(
                        !sddl.contains(principal),
                        "{case}: DACL still grants {principal}: {sddl}",
                    );
                }
            };

            let base = std::env::temp_dir().join(format!(
                "scrib-dacl-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&base);

            // Fresh creation: never open for an instant.
            let fresh = base.join("fresh");
            create_state_dir(&fresh).expect("create");
            assert_private(&fresh, "freshly created");

            // Migration: a directory an earlier build created with a bare
            // `create_dir_all`, inheriting whatever the parent granted.
            let stale = base.join("stale");
            std::fs::create_dir_all(&stale).unwrap();
            create_state_dir(&stale).expect("create over an existing directory");
            assert_private(&stale, "pre-existing directory");

            let _ = std::fs::remove_dir_all(&base);
        }
        #[cfg(all(not(unix), not(windows)))]
        {
            crate::testsymlink::skipped(
                "TDD 21.12 state-directory permissions",
                "neither POSIX modes nor Windows ACLs apply on this platform",
            );
        }
        #[cfg(unix)]
        {
            use super::create_state_dir;
            use std::os::unix::fs::PermissionsExt;
            use std::path::Path;
            let tmp = tempfile::tempdir().unwrap();
            let mode_of = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;

            // Freshly created, including intermediate components.
            let fresh = tmp.path().join("state").join("scribobulate");
            create_state_dir(&fresh).expect("create");
            assert_eq!(
                mode_of(&fresh),
                0o700,
                "a new state directory must not be readable by anyone else — it holds \
                 session.toml, which records the path of every open document"
            );

            // An existing directory left open by an earlier build is tightened.
            let stale = tmp.path().join("stale");
            std::fs::create_dir_all(&stale).unwrap();
            std::fs::set_permissions(&stale, std::fs::Permissions::from_mode(0o775)).unwrap();
            assert_eq!(mode_of(&stale), 0o775, "precondition: the pre-fix mode");
            create_state_dir(&stale).expect("create over an existing directory");
            assert_eq!(
                mode_of(&stale),
                0o700,
                "an installation that has already run keeps its open directory forever \
                 unless creation also narrows an existing one"
            );

            // And the point of doing this at the directory: a file inside keeps whatever
            // mode its own writer chose (write_atomic deliberately relaxes new files, so
            // that documents are not forced to 0600) and is still unreachable to others,
            // because reaching it requires traversing this directory.
            let inside = fresh.join("session.toml");
            std::fs::write(&inside, "x").unwrap();
            assert_eq!(
                mode_of(fresh.as_path()) & 0o077,
                0,
                "no group/other bits on the directory is what protects {}",
                inside.display()
            );
        }
    }

    /// The state directory must resolve on EVERY supported platform from the real
    /// environment — no `XDG_STATE_HOME` override in sight (GEP-42).
    ///
    /// This is the shape of test that would actually have caught it. A
    /// `#[cfg(windows)]` test asserting on a path would not have: the failure was
    /// that no path was produced *at all*, silently, because the only fallback was
    /// `HOME` — a POSIX convention Windows does not set. Every other session test
    /// goes through `with_state_home_for_test`, which sets `XDG_STATE_HOME` and so
    /// masks exactly this bug.
    #[test]
    fn state_directory_resolves_without_any_xdg_override() {
        let _g = super::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var_os("XDG_STATE_HOME");
        std::env::remove_var("XDG_STATE_HOME");
        let resolved = session_path();
        match prev {
            Some(v) => std::env::set_var("XDG_STATE_HOME", v),
            None => std::env::remove_var("XDG_STATE_HOME"),
        }
        assert!(
            resolved.is_some(),
            "with XDG_STATE_HOME unset the platform fallback must still yield a \
             state path; returning None here means session save/load silently \
             no-ops and nothing persists (GEP-42)"
        );
    }
}
