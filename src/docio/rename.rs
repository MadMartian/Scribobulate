//! Renaming a document's file, in place, within its own directory.
//!
//! # Why this is not `std::fs::rename`, and not a bare GIO call either
//!
//! `std::fs::rename` silently replaces an existing destination, so the obvious
//! implementation — check the target is free, then rename — destroys an unrelated
//! file when it loses the race, with no error and no log line.
//!
//! **GIO does not fix that, and it is important not to believe it does.**
//! `g_file_set_display_name` and `g_file_move` both `g_lstat()` the destination and
//! then call a plain `g_rename()`, with nothing in between (`glocalfile.c:1158-1191`
//! and `:2467-2532`); a tree-wide grep for `renameat2` / `RENAME_NOREPLACE` /
//! `renamex_np` finds **zero hits** in GLib 2.72.4 *or* in `main`. On Windows it is
//! worse: `gstdio.c:1175` passes `MOVEFILE_REPLACE_EXISTING` unconditionally, so
//! that one `lstat` is the whole of the protection. **This module therefore narrows
//! a race it does not close, and says so rather than claiming a refusal.**
//! (SOURCE-TRACED with verbatim quotes, researcher 2026-08-15.)
//!
//! An atomic no-replace rename does exist on all three platforms —
//! `renameat2(RENAME_NOREPLACE)` (MEASURED refusing over an existing destination on
//! ext4 and tmpfs, Linux/glibc 2.35), `renamex_np(RENAME_EXCL)`, `MoveFileExW(…, 0)`
//! — and GLib uses none of them. Taking that path means three-platform unsafe FFI
//! for a command that renames inside the user's own directory, which is
//! disproportionate; [`rename_blocking`] is the single place that would change if
//! that judgement is ever revisited.
//!
//! `set_display_name` is chosen over `move_` on three counts: it is the
//! rename-within-parent operation rather than a general move, it carries *some*
//! separator guard where `move_` carries none, and its async form predates our GTK
//! 2.72 floor by a decade (`g_file_move_async` is *Since: 2.72* — exactly the floor,
//! with no headroom).
//!
//! # Why the name is validated here rather than left to GIO
//!
//! Two independent reasons, and neither implies the other:
//!
//! 1. **GIO's separator guard does not enforce "same directory" on Windows.** It is
//!    a single-character `strchr` for `G_DIR_SEPARATOR` in the public wrapper
//!    (`gfile.c:4423-4430`), which is `\` there — while GLib's own path machinery
//!    also honours `/` (`gfileutils.h:191`) and resolves `..` lexically below the
//!    guard. So `sub/x.md` and `../x.md` escape the directory on Windows.
//! 2. **GIO reports `""`, `.` and `..` as `G_IO_ERROR_EXISTS`** — "filename already
//!    exists", shown to a reader who typed nothing. `InvalidFilename` only appears
//!    when `rename(2)` itself returns `EINVAL`, essentially FAT-only.
//!
//! The rules differ per filesystem, so they are taken as **data** rather than read
//! from `cfg!` — the same reason `accel.rs` takes a `Platform`: a pure function over
//! plain data is checkable for *every* platform from any host, which a `#[cfg]`-gated
//! one never is (POLICY forbids `#[cfg]`ing a test because that deletes it).

use std::path::{Path, PathBuf};

/// Which filesystem's naming rules to apply.
///
/// Deliberately **not** [`crate::accel::Platform`], whose `Other` lumps Linux and
/// Windows together — that is the correct split for accelerators and exactly the
/// wrong one here, since Windows is the platform with the extra rules.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum FsRules {
    /// Linux and macOS: only the separator, NUL, and the `.`/`..` entries are illegal.
    Posix,
    /// Windows: additionally reserved device names, a trailing dot or space, the
    /// `< > : " | ? *` set, and `/` as a second separator.
    Windows,
}

/// The rules for the platform this binary was built for.
pub(crate) const fn host_rules() -> FsRules {
    if cfg!(windows) {
        FsRules::Windows
    } else {
        FsRules::Posix
    }
}

/// Why a proposed filename cannot be used. Carries no formatting of its own beyond
/// [`std::fmt::Display`], so the dialog and the tests read the same words.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum NameRefusal {
    Empty,
    /// `.` or `..` — directory entries, not names a file can take.
    DotEntry,
    /// Contains `/` (either platform) or `\` (Windows).
    Separator,
    /// An interior NUL, which no filesystem accepts and which truncates a C string.
    InteriorNul,
    /// The name the document already has — a rename to itself is not a rename.
    /// Distinct from [`Self::DotEntry`] and friends because it is not *illegal*, it
    /// is simply a no-op, and the dialog says so differently.
    Unchanged,
    /// Windows: `CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9`, `LPT1`-`LPT9`, with or
    /// without an extension.
    ReservedDeviceName,
    /// Windows: a trailing `.` or space, which the shell silently strips.
    TrailingDotOrSpace,
    /// Windows: one of `< > : " | ? *`.
    ForbiddenCharacter(char),
}

impl std::fmt::Display for NameRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "Enter a filename."),
            Self::DotEntry => write!(f, "“.” and “..” are not filenames."),
            Self::Separator => write!(
                f,
                "A filename cannot contain a path separator — Rename changes the \
                 name, not the folder."
            ),
            Self::InteriorNul => write!(f, "A filename cannot contain a null character."),
            Self::Unchanged => write!(f, "That is already the document's name."),
            Self::ReservedDeviceName => {
                write!(f, "That name is reserved by Windows for a device.")
            }
            Self::TrailingDotOrSpace => {
                write!(f, "A filename cannot end with a dot or a space on Windows.")
            }
            Self::ForbiddenCharacter(c) => {
                write!(f, "A filename cannot contain “{c}” on Windows.")
            }
        }
    }
}

/// What kind of rename a validated name asks for.
///
/// Returned by [`validate_new_name`] rather than computed separately, so a caller
/// cannot validate a name and then forget to ask whether it is the case-only case
/// that needs the two-step. One decision, one answer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum NameChange {
    /// An ordinary rename to a different name.
    Plain,
    /// The same name in different letter case (`notes.md` → `Notes.md`).
    ///
    /// On a case-**insensitive** filesystem (APFS, NTFS) the destination "exists" —
    /// it *is* the source — so GIO's `lstat` refuses a legitimate rename. Neither
    /// primitive compares `st_dev`/`st_ino`, so there is nothing to opt into; the
    /// rename has to go through a temp name. Safe on a case-sensitive filesystem
    /// too, where it is simply two renames instead of one.
    CaseOnly,
}

/// Windows device names, reserved with or without an extension (`CON.md` too).
const WINDOWS_DEVICE_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Characters Windows forbids in a filename, beyond the separators.
const WINDOWS_FORBIDDEN: &[char] = &['<', '>', ':', '"', '|', '?', '*'];

/// Decide whether `proposed` is a usable new name for a document currently called
/// `current`, and which kind of rename it asks for.
///
/// Pure and display-free: the dialog calls it on every keystroke to gate its confirm
/// control, and the operation calls it again before touching the filesystem. Both
/// readers, one rule (TDD 24.9).
pub(crate) fn validate_new_name(
    current: &str,
    proposed: &str,
    rules: FsRules,
) -> Result<NameChange, NameRefusal> {
    if proposed.is_empty() {
        return Err(NameRefusal::Empty);
    }
    if proposed == "." || proposed == ".." {
        return Err(NameRefusal::DotEntry);
    }
    if proposed.contains('\0') {
        return Err(NameRefusal::InteriorNul);
    }
    // `/` is a separator on BOTH platforms — GLib's own path code honours it on
    // Windows even though `G_DIR_SEPARATOR` is `\` there, which is precisely the
    // hole GIO's one-character guard leaves open.
    if proposed.contains('/') || (rules == FsRules::Windows && proposed.contains('\\')) {
        return Err(NameRefusal::Separator);
    }
    if rules == FsRules::Windows {
        if let Some(c) = proposed.chars().find(|c| WINDOWS_FORBIDDEN.contains(c)) {
            return Err(NameRefusal::ForbiddenCharacter(c));
        }
        if proposed.ends_with('.') || proposed.ends_with(' ') {
            return Err(NameRefusal::TrailingDotOrSpace);
        }
        let stem = proposed.split('.').next().unwrap_or(proposed);
        if WINDOWS_DEVICE_NAMES
            .iter()
            .any(|d| d.eq_ignore_ascii_case(stem))
        {
            return Err(NameRefusal::ReservedDeviceName);
        }
    }
    if proposed == current {
        return Err(NameRefusal::Unchanged);
    }
    if is_case_only_change(current, proposed) {
        return Ok(NameChange::CaseOnly);
    }
    Ok(NameChange::Plain)
}

/// Whether two names differ only by letter case.
///
/// ASCII-only folding, deliberately. Full Unicode case folding would make the
/// *predicate* more general without making the *operation* more correct: the
/// two-step path it selects is safe for any rename, so a missed non-ASCII case-only
/// change degrades to an ordinary rename — which is right on a case-sensitive
/// filesystem and refused with a clear "already exists" on a case-insensitive one,
/// rather than silently doing the wrong thing.
fn is_case_only_change(a: &str, b: &str) -> bool {
    a != b && a.eq_ignore_ascii_case(b)
}

/// Why a rename did not happen. Discriminated from the `GIOErrorEnum` **code**,
/// never from the message — every case shares the `g-io-error-quark` domain and the
/// messages are translated, so a message match would be a per-locale bug.
#[derive(Clone, Debug)]
pub(crate) enum RenameError {
    /// Refused before the filesystem was touched (TDD 24.9).
    Invalid(NameRefusal),
    /// A file of that name is already there. **Its contents are untouched** — GIO
    /// refuses before renaming (TDD 24.7).
    DestinationExists,
    /// The document's own file is gone. The caller flips the tab into its
    /// backing-missing state so Save can re-create it (TDD 24.8).
    SourceMissing,
    /// Anything else — permissions, a read-only filesystem, an I/O failure.
    Other(String),
}

impl std::fmt::Display for RenameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(r) => write!(f, "{r}"),
            Self::DestinationExists => {
                write!(f, "A file with that name already exists in this folder.")
            }
            Self::SourceMissing => write!(f, "This document's file no longer exists on disk."),
            Self::Other(msg) => write!(f, "{msg}"),
        }
    }
}

/// Map a `glib::Error` from a rename call onto [`RenameError`], by code.
fn classify(err: &gtk::glib::Error) -> RenameError {
    use gtk::gio::IOErrorEnum;
    match err.kind::<IOErrorEnum>() {
        Some(IOErrorEnum::Exists) => RenameError::DestinationExists,
        Some(IOErrorEnum::NotFound) => RenameError::SourceMissing,
        _ => RenameError::Other(err.message().to_string()),
    }
}

/// A temp name for the two-step case-only rename, in the document's own directory.
///
/// The `.rename-` infix is the recognisable marker for the orphan a crash between
/// the two steps would leave; the counter makes two concurrent renames in one
/// directory impossible to collide, which the pid alone would not.
fn temp_name_for(current: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    format!(
        "{current}.rename-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

/// Rename `path`'s file to `new_name`, in the same directory. The blocking half —
/// runs on GLib's I/O pool, never the main thread.
fn rename_blocking(path: &Path, new_name: &str, rules: FsRules) -> Result<PathBuf, RenameError> {
    use gtk::gio::prelude::FileExt;

    let current = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let change = validate_new_name(&current, new_name, rules).map_err(RenameError::Invalid)?;

    let file = gtk::gio::File::for_path(path);
    let target = path.with_file_name(new_name);

    match change {
        NameChange::Plain => {
            file.set_display_name(new_name, gtk::gio::Cancellable::NONE)
                .map_err(|e| classify(&e))?;
        }
        NameChange::CaseOnly => {
            // Two steps, because the refusal being stepped around is GIO's own and
            // cannot be disabled. Each step is individually atomic with respect to
            // readers; **the pair is not**, so a crash between them leaves the temp
            // name behind — which is why it is recognisably `<name>.rename-*`.
            let temp = temp_name_for(&current);
            let midway = file
                .set_display_name(&temp, gtk::gio::Cancellable::NONE)
                .map_err(|e| classify(&e))?;
            if let Err(e) = midway.set_display_name(new_name, gtk::gio::Cancellable::NONE) {
                // Roll the first step back so a failed case-only rename leaves the
                // document under the name it started with, rather than under the
                // temp name — where the tab would be pointing at a file the reader
                // cannot recognise. Best-effort: if the rollback itself fails there
                // is nothing further to try, and the original error is the one worth
                // reporting.
                let _ = midway.set_display_name(&current, gtk::gio::Cancellable::NONE);
                return Err(classify(&e));
            }
        }
    }
    Ok(target)
}

/// Rename the document at `path` to `new_name`, off the main thread.
///
/// Returns the new path on success. **The destination check is not atomic** — see
/// this module's header; the refusal is GIO's `lstat`-then-`rename`, so a file
/// created in the window between them is silently replaced. Documented rather than
/// claimed away.
pub(crate) async fn rename_document(
    path: PathBuf,
    new_name: String,
) -> Result<PathBuf, RenameError> {
    super::pool::off_main(move || rename_blocking(&path, &new_name, host_rules())).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The universal refusals — the ones that hold on every platform, and the reason
    /// the app validates at all rather than letting GIO answer: GIO reports the first
    /// three of these as `Exists`, i.e. "filename already exists" to a reader who
    /// typed nothing.
    #[test]
    fn a_name_that_cannot_be_a_filename_is_refused_on_every_platform() {
        for rules in [FsRules::Posix, FsRules::Windows] {
            assert_eq!(
                validate_new_name("a.md", "", rules),
                Err(NameRefusal::Empty),
                "{rules:?}"
            );
            assert_eq!(
                validate_new_name("a.md", ".", rules),
                Err(NameRefusal::DotEntry),
                "{rules:?}"
            );
            assert_eq!(
                validate_new_name("a.md", "..", rules),
                Err(NameRefusal::DotEntry),
                "{rules:?}"
            );
            assert_eq!(
                validate_new_name("a.md", "b\0c.md", rules),
                Err(NameRefusal::InteriorNul),
                "{rules:?}"
            );
            // The whole point of the feature's scope: a rename changes the name, so
            // anything that could change the DIRECTORY is refused.
            assert_eq!(
                validate_new_name("a.md", "sub/b.md", rules),
                Err(NameRefusal::Separator),
                "{rules:?}"
            );
            assert_eq!(
                validate_new_name("a.md", "../b.md", rules),
                Err(NameRefusal::Separator),
                "{rules:?}"
            );
            assert_eq!(
                validate_new_name("a.md", "a.md", rules),
                Err(NameRefusal::Unchanged),
                "{rules:?}"
            );
        }
    }

    /// **The Windows rules are asserted from Linux**, which is the whole reason the
    /// rules are a parameter. `#[cfg(windows)]` here would not skip these on Linux,
    /// it would delete them (POLICY § Unit tests, ScrAP-212) — and Windows is the
    /// platform whose extra rules nobody would otherwise exercise.
    #[test]
    fn windows_adds_rules_that_posix_does_not_have() {
        // A backslash is a separator on Windows and an ordinary (if eccentric)
        // filename character on POSIX.
        assert_eq!(
            validate_new_name("a.md", "sub\\b.md", FsRules::Windows),
            Err(NameRefusal::Separator)
        );
        assert_eq!(
            validate_new_name("a.md", "sub\\b.md", FsRules::Posix),
            Ok(NameChange::Plain),
            "a backslash is a legal POSIX filename character — refusing it here \
             would reject a legitimate rename on Linux and macOS"
        );

        // Reserved device names, with and without an extension.
        for name in ["CON", "con.md", "LPT9.txt", "NUL"] {
            assert_eq!(
                validate_new_name("a.md", name, FsRules::Windows),
                Err(NameRefusal::ReservedDeviceName),
                "{name}"
            );
            assert_eq!(
                validate_new_name("a.md", name, FsRules::Posix),
                Ok(NameChange::Plain),
                "{name} is an ordinary name on POSIX"
            );
        }
        // COM0 is NOT reserved — the reserved set is COM1-9, and a table that
        // over-reaches refuses legitimate names.
        assert_eq!(
            validate_new_name("a.md", "COM0.md", FsRules::Windows),
            Ok(NameChange::Plain)
        );

        assert_eq!(
            validate_new_name("a.md", "b.md.", FsRules::Windows),
            Err(NameRefusal::TrailingDotOrSpace)
        );
        assert_eq!(
            validate_new_name("a.md", "b.md ", FsRules::Windows),
            Err(NameRefusal::TrailingDotOrSpace)
        );
        assert_eq!(
            validate_new_name("a.md", "a:b.md", FsRules::Windows),
            Err(NameRefusal::ForbiddenCharacter(':'))
        );
        assert_eq!(
            validate_new_name("a.md", "a?b.md", FsRules::Windows),
            Err(NameRefusal::ForbiddenCharacter('?'))
        );
        assert_eq!(
            validate_new_name("a.md", "a:b.md", FsRules::Posix),
            Ok(NameChange::Plain),
            "a colon is legal on POSIX"
        );
    }

    /// A case-only change is a *legitimate rename* that must be recognised as its own
    /// kind, because on APFS/NTFS the destination "exists" — it is the source — and
    /// GIO's bare `lstat` refuses it. The two-step is selected here, not at the call
    /// site (TDD 24.10).
    #[test]
    fn a_case_only_change_is_recognised_as_its_own_kind_of_rename() {
        assert_eq!(
            validate_new_name("notes.md", "Notes.md", FsRules::Posix),
            Ok(NameChange::CaseOnly)
        );
        assert_eq!(
            validate_new_name("NOTES.MD", "notes.md", FsRules::Windows),
            Ok(NameChange::CaseOnly)
        );
        // Same letters, different name → an ordinary rename, not the case path.
        assert_eq!(
            validate_new_name("notes.md", "Note.md", FsRules::Posix),
            Ok(NameChange::Plain)
        );
        // Identical is not a case-only change; it is no change at all, and the
        // dialog must say that rather than offering a rename that GIO would answer
        // with "already exists".
        assert_eq!(
            validate_new_name("notes.md", "notes.md", FsRules::Posix),
            Err(NameRefusal::Unchanged)
        );
    }

    #[test]
    fn case_only_detection_is_exact() {
        assert!(is_case_only_change("a.md", "A.md"));
        assert!(
            !is_case_only_change("a.md", "a.md"),
            "identical is not a change"
        );
        assert!(!is_case_only_change("a.md", "b.md"));
    }

    /// The temp name is recognisable as this app's rename debris and unique per
    /// call — two concurrent renames in one directory must not collide on it.
    #[test]
    fn the_two_step_temp_name_is_recognisable_and_unique() {
        let a = temp_name_for("notes.md");
        let b = temp_name_for("notes.md");
        assert!(a.starts_with("notes.md.rename-"), "{a}");
        assert_ne!(a, b, "two renames in one directory must not collide");
    }

    /// Every refusal renders a sentence a reader can act on — no `Debug` output and
    /// no empty string reaching the dialog.
    #[test]
    fn every_refusal_explains_itself() {
        for r in [
            NameRefusal::Empty,
            NameRefusal::DotEntry,
            NameRefusal::Separator,
            NameRefusal::InteriorNul,
            NameRefusal::Unchanged,
            NameRefusal::ReservedDeviceName,
            NameRefusal::TrailingDotOrSpace,
            NameRefusal::ForbiddenCharacter('?'),
        ] {
            let msg = r.to_string();
            assert!(msg.len() > 10, "{r:?} → {msg:?}");
            assert!(msg.ends_with('.'), "{r:?} → {msg:?}");
        }
    }

    /// The real filesystem, through the real GIO calls, with no display and no main
    /// loop — the blocking half is where every decision lives, so this is where the
    /// behaviour is pinned (the async skin only chooses the thread).
    #[test]
    fn a_plain_rename_moves_the_file_within_its_directory() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("before.md");
        std::fs::write(&old, "# body\n").unwrap();

        let new = rename_blocking(&old, "after.md", FsRules::Posix).expect("the rename succeeds");

        assert_eq!(new, dir.path().join("after.md"));
        assert!(!old.exists(), "the old name is gone");
        assert_eq!(
            std::fs::read_to_string(&new).unwrap(),
            "# body\n",
            "the bytes are unchanged — a rename is not a rewrite (TDD 24.1)"
        );
    }

    /// TDD 24.7 — and note what is asserted: the victim's **contents**, not merely
    /// that an error came back. The failure this guards is a rename that reports
    /// success having destroyed an unrelated file.
    #[test]
    fn an_existing_destination_is_refused_and_left_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("mine.md");
        let victim = dir.path().join("theirs.md");
        std::fs::write(&old, "mine\n").unwrap();
        std::fs::write(&victim, "theirs\n").unwrap();

        let err = rename_blocking(&old, "theirs.md", FsRules::Posix)
            .expect_err("an occupied destination must be refused");
        assert!(matches!(err, RenameError::DestinationExists), "{err:?}");

        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "theirs\n");
        assert_eq!(std::fs::read_to_string(&old).unwrap(), "mine\n");
    }

    /// TDD 24.8 — the caller distinguishes this from every other failure in order to
    /// flip the tab into its backing-missing state, so the classification matters and
    /// is asserted by variant rather than by message.
    #[test]
    fn a_vanished_source_is_reported_as_such() {
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join("never-existed.md");
        let err =
            rename_blocking(&gone, "other.md", FsRules::Posix).expect_err("nothing to rename");
        assert!(matches!(err, RenameError::SourceMissing), "{err:?}");
    }

    /// The two-step path runs on Linux too, where it is simply two renames. Running
    /// it here is what keeps it from being code only macOS and Windows ever execute —
    /// the shape that rots unnoticed (TDD 24.10).
    #[test]
    fn a_case_only_rename_completes_and_leaves_no_debris() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("notes.md");
        std::fs::write(&old, "# notes\n").unwrap();

        let new =
            rename_blocking(&old, "Notes.md", FsRules::Posix).expect("a case-only rename succeeds");
        assert_eq!(new, dir.path().join("Notes.md"));
        assert_eq!(std::fs::read_to_string(&new).unwrap(), "# notes\n");

        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".rename-"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "the temp name must not survive a successful two-step: {leftovers:?}"
        );
    }

    /// Validation runs **before** the filesystem is touched, so an illegal name
    /// cannot traverse out of the directory even on a platform whose GIO guard would
    /// have let it (the Windows hole). Asserted by checking the parent directory is
    /// untouched, not just that an error was returned.
    #[test]
    fn an_illegal_name_never_reaches_the_filesystem() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("doc");
        std::fs::create_dir(&sub).unwrap();
        let old = sub.join("a.md");
        std::fs::write(&old, "body\n").unwrap();

        for bad in ["../escaped.md", "sub/nested.md", "", ".."] {
            let err = rename_blocking(&old, bad, FsRules::Posix)
                .expect_err(&format!("{bad:?} must be refused"));
            assert!(
                matches!(err, RenameError::Invalid(_)),
                "{bad:?} must be refused by OUR validator, before GIO sees it — got {err:?}"
            );
        }
        assert!(old.exists(), "the document keeps its name");
        assert!(
            !dir.path().join("escaped.md").exists(),
            "nothing may appear in the PARENT directory — this is the traversal the \
             Windows GIO guard does not stop"
        );
    }
}
