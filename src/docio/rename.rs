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
//! ext4 and tmpfs, Linux/glibc 2.35), `renamex_np(RENAME_EXCL)` and
//! `MoveFileExW(…, 0)` (both **DOC-ASSERTED only**: taken from the platform
//! documentation, never measured, and nobody should quote them as tested)
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

/// Is `entry` the debris [`temp_name_for`] would have produced for `document`?
///
/// Matched against the full shape — `<document>.rename-<digits>-<digits>` — rather
/// than on the `.rename-` infix alone, because the infix is a substring a user's own
/// file may legitimately contain (`notes.md.rename-plan.md` is a document, not
/// debris) and the recovery below *moves* whatever this admits.
fn is_rename_orphan_of(entry: &str, document: &str) -> bool {
    let Some(tail) = entry.strip_prefix(document) else {
        return false;
    };
    let Some(tail) = tail.strip_prefix(".rename-") else {
        return false;
    };
    let Some((pid, seq)) = tail.split_once('-') else {
        return false;
    };
    !pid.is_empty()
        && !seq.is_empty()
        && pid.bytes().all(|b| b.is_ascii_digit())
        && seq.bytes().all(|b| b.is_ascii_digit())
}

/// Put back a document that a crash stranded midway through a case-only rename.
///
/// ScrAP-272: naming the debris `recognisably` is a property of a string, and the
/// obligation is the verb — this is the recogniser that was missing.
///
/// The two-step case-only rename is atomic in each step and **not** across the pair,
/// so a crash between them leaves the document under `<name>.rename-<pid>-<seq>` and
/// *nothing at all* under either the old name or the new one. Without this the file
/// is not damaged, but it is invisible: the app reports the document missing and
/// offers to create a new empty one over it, and the user's text survives only under
/// a name nothing points at.
///
/// Recovery is deliberately narrow, because it moves a file the user did not ask it
/// to move:
///
/// * **Only when `path` itself is absent.** A present document is the document; a
///   stray orphan beside it is debris this must not touch.
/// * **Only when exactly one orphan matches.** Two mean either a second crash or a
///   stale one from an earlier run, and which of them is the document is not knowable
///   from the names — so neither is moved and both are left where a user can see them.
/// * **Only back to the name the orphan encodes.** The target is `<document>`, which
///   is the *pre*-rename name recorded in the orphan's own spelling; the rename the
///   user asked for is not replayed, since a crash is no evidence they still want it.
///
/// Returns the restored path, or `None` when nothing was recovered — which is the
/// overwhelmingly common case and is not an error.
pub(super) fn recover_rename_orphan(path: &Path) -> Option<PathBuf> {
    // The "only when `path` is absent" rule lives HERE, not only at the call site
    // that already checks it. A guard that a caller has to remember is one refactor
    // away from being a silent overwrite of a document that was never missing.
    if path.exists() {
        return None;
    }
    let document = path.file_name()?.to_str()?;
    let dir = path.parent()?;

    let mut found: Option<PathBuf> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !is_rename_orphan_of(name, document) {
            continue;
        }
        if let Some(first) = &found {
            log::warn!(
                "rename recovery: {} has more than one orphan ({} and {}) — recovering neither",
                path.display(),
                first.display(),
                entry.path().display()
            );
            return None;
        }
        found = Some(entry.path());
    }

    let orphan = found?;
    // `std::fs::rename` replaces an existing destination silently, which is the very
    // hazard this module exists to narrow — but the destination here was just observed
    // absent by the caller, and re-creating it in the window between is the same
    // residual TOCTOU the module header already documents rather than claims away.
    match std::fs::rename(&orphan, path) {
        Ok(()) => {
            log::info!(
                "rename recovery: restored {} from the orphan {}",
                path.display(),
                orphan.display()
            );
            Some(path.to_path_buf())
        }
        Err(e) => {
            log::warn!(
                "rename recovery: could not restore {} from {}: {e}",
                path.display(),
                orphan.display()
            );
            None
        }
    }
}

/// The byte-exact name the filesystem actually stored for `renamed`, when that is
/// not the name it was asked for. `None` means the requested spelling is the
/// authoritative one — which is every rename on ext4, on NTFS, and on APFS.
///
/// **`set_display_name` never re-reads what landed on disk**; it reports back the
/// name it was given, and so does the `GFile` it returns. SOURCE-READ, not inferred:
/// `g_local_file_set_display_name` builds its result with
/// `g_file_get_child_for_display_name(parent, display_name, …)` *before* the `lstat`
/// and *before* the `g_rename`, and returns it unmodified — a scan for any
/// post-rename re-read inside that function finds nothing at any tag from 2.72.4 to
/// 2.88.0. The returned path is exactly `parent.join(display_name)`, canonicalised.
/// On Windows it additionally comes back **backslash-separated** and carrying the
/// case the user typed, so do not compare it against a `/`-separated string and do
/// not read its case as verified. On a normalising
/// filesystem those differ: **HFS+** decomposes to NFD, so a name typed as NFC is
/// stored decomposed. Be precise about which filesystem that is — an earlier
/// version of this comment let "macOS" stand in for HFS+, and it is **APFS** that
/// current Macs run. Both halves are now MEASURED (mac seat, macOS 26.6.1,
/// `od -c` on the stored entry, NFC written deliberately rather than typed):
/// APFS is normalization-**preserving** and hands back `c a f 303 251` untouched,
/// while a purpose-built HFS+ image decomposes the same input to
/// `c a f e 314 201`. So this function's normalising audience is HFS+ volumes —
/// external disks, Time Machine targets, older systems — and not the Mac in front
/// of you. It stays because "the volume is HFS+" is not something a rename may
/// assume either way, and because the cost is one enumeration on a path that just
/// changed. Every consumer of the new path — the tab, the window title,
/// the Documents list, and above all the file monitor the caller is about to
/// re-attach — would then be keyed on a spelling no directory entry matches, which
/// is the "re-verify on re-attach" obligation failing at its first step.
///
/// A `FileEnumerator` is the authority here and `query_info` is not: `standard::name`
/// from a `query_info` is derived from the `GFile`'s own path, so it echoes the
/// question back (ScrAP-270 — MEASURED: a `query_info` that *follows* a symlink
/// returns the link's own name beside the target's `id::file`). Identity is matched
/// on `id::file` rather than by comparing spellings, which keeps this free of any
/// Unicode-normalisation logic of our own — the filesystem is asked which entry *is*
/// this file, not which entry looks like it. That identity is **not unique among
/// directory entries**, though: hard links share it, which is ScrAP-271 and why the
/// scan below finishes before it answers.
///
/// Best-effort by construction: every failure yields `None` and the caller keeps the
/// requested spelling, which is what it would have used anyway. **A rename that
/// succeeded must never be reported as failed because a follow-up read did.**
fn stored_spelling(renamed: &gtk::gio::File, requested: &str) -> Option<String> {
    use gtk::gio::prelude::*;

    let parent = renamed.parent()?;
    let want = renamed
        .query_info(
            "id::file",
            gtk::gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            gtk::gio::Cancellable::NONE,
        )
        .ok()?
        .attribute_string("id::file")?;

    let children = parent
        .enumerate_children(
            "standard::name,id::file",
            gtk::gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            gtk::gio::Cancellable::NONE,
        )
        .ok()?;
    // An identity match is only the answer once the WHOLE directory has failed to
    // produce the requested spelling — not the moment one is seen. A hard link is a
    // second name for the same file and so carries the same `id::file`, and `readdir`
    // order is unspecified, so answering from the first match would substitute an
    // alias's name for a name the directory holds perfectly well, on some orderings
    // and not others. Returning early on `requested` is still correct and still the
    // fast path: that one is decisive on sight.
    let mut identity: Option<String> = None;
    for info in children.flatten() {
        let entry = info.name();
        let Some(entry) = entry.to_str() else {
            continue;
        };
        if entry == requested {
            return None;
        }
        if identity.is_none() && info.attribute_string("id::file").as_deref() == Some(want.as_str())
        {
            identity = Some(entry.to_owned());
        }
    }
    identity
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

    // The `GFile` each call returns is the renamed file — kept, not discarded, because
    // it is what `stored_spelling` interrogates below.
    let renamed = match change {
        NameChange::Plain => file
            .set_display_name(new_name, gtk::gio::Cancellable::NONE)
            .map_err(|e| classify(&e))?,
        NameChange::CaseOnly => {
            // Two steps, because the refusal being stepped around is GIO's own and
            // cannot be disabled. Each step is individually atomic with respect to
            // readers; **the pair is not**, so a crash between them leaves the temp
            // name behind — which is why it is recognisably `<name>.rename-*`, and
            // why `recover_rename_orphan` exists to put it back.
            let temp = temp_name_for(&current);
            let midway = file
                .set_display_name(&temp, gtk::gio::Cancellable::NONE)
                .map_err(|e| classify(&e))?;
            match midway.set_display_name(new_name, gtk::gio::Cancellable::NONE) {
                Ok(renamed) => renamed,
                Err(e) => {
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
    };

    Ok(match stored_spelling(&renamed, new_name) {
        Some(stored) => {
            log::info!("rename: filesystem stored {stored:?} for the requested {new_name:?}");
            path.with_file_name(stored)
        }
        None => path.with_file_name(new_name),
    })
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

    /// The orphan matcher admits exactly what `temp_name_for` emits and nothing that
    /// merely resembles it. The false-positive half is the load-bearing one: recovery
    /// *moves* what this admits, so a user's own file that happens to contain
    /// `.rename-` must not qualify.
    #[test]
    fn only_this_apps_rename_debris_is_recognised_as_an_orphan() {
        // Round-trip against the real producer, so the two cannot drift apart.
        assert!(is_rename_orphan_of(&temp_name_for("notes.md"), "notes.md"));

        assert!(is_rename_orphan_of("notes.md.rename-1234-0", "notes.md"));
        assert!(
            !is_rename_orphan_of("notes.md.rename-1234-0", "other.md"),
            "debris belongs to the document whose name it carries"
        );
        for not_debris in [
            "notes.md.rename-plan.md", // a document a user could plausibly have
            "notes.md.rename-",        // no pid, no seq
            "notes.md.rename-1234",    // no seq
            "notes.md.rename-a-0",     // pid is not digits
            "notes.md.rename-1234-x",  // seq is not digits
            "notes.md",                // the document itself
        ] {
            assert!(
                !is_rename_orphan_of(not_debris, "notes.md"),
                "{not_debris:?} must not be treated as debris"
            );
        }
    }

    /// The crash this recovers from: the first step of a case-only rename landed, the
    /// second never ran, and the document exists under neither name the user knows.
    #[test]
    fn a_document_stranded_by_a_crash_mid_rename_is_put_back() {
        let dir = tempfile::tempdir().unwrap();
        let doc = dir.path().join("notes.md");
        let orphan = dir.path().join(temp_name_for("notes.md"));
        std::fs::write(&orphan, "# survived\n").unwrap();

        let restored = recover_rename_orphan(&doc).expect("the orphan is recovered");

        assert_eq!(restored, doc);
        assert_eq!(std::fs::read_to_string(&doc).unwrap(), "# survived\n");
        assert!(!orphan.exists(), "the orphan is consumed, not copied");
    }

    /// Recovery must never overwrite a document that is simply there — the orphan
    /// beside it is debris, and the file on disk is the truth.
    #[test]
    fn an_orphan_beside_a_present_document_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let doc = dir.path().join("notes.md");
        let orphan = dir.path().join(temp_name_for("notes.md"));
        std::fs::write(&doc, "# the real one\n").unwrap();
        std::fs::write(&orphan, "# debris\n").unwrap();

        assert!(recover_rename_orphan(&doc).is_none());
        assert_eq!(
            std::fs::read_to_string(&doc).unwrap(),
            "# the real one\n",
            "the present document must survive untouched"
        );
    }

    /// Two orphans mean two crashes, and nothing in the names says which one is the
    /// document. Recovering the wrong one would silently resurrect stale content over
    /// the newer, so neither is moved.
    #[test]
    fn an_ambiguous_pair_of_orphans_recovers_neither() {
        let dir = tempfile::tempdir().unwrap();
        let doc = dir.path().join("notes.md");
        let first = dir.path().join("notes.md.rename-100-0");
        let second = dir.path().join("notes.md.rename-200-0");
        std::fs::write(&first, "one\n").unwrap();
        std::fs::write(&second, "two\n").unwrap();

        assert!(recover_rename_orphan(&doc).is_none());
        assert!(!doc.exists(), "nothing is put back");
        assert!(first.exists() && second.exists(), "both are left in place");
    }

    /// The path a rename reports must be the path a directory listing agrees with.
    /// On ext4/NTFS that is the requested spelling by construction; on a normalising
    /// filesystem it is `stored_spelling`'s answer instead. Asserting it against the
    /// directory rather than against the input is what makes the same test meaningful
    /// on all three platforms (TDD 24.10's shape).
    #[test]
    fn the_reported_path_is_the_one_the_directory_actually_holds() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("before.md");
        std::fs::write(&old, "# body\n").unwrap();

        let new = rename_blocking(&old, "after.md", FsRules::Posix).expect("the rename succeeds");

        let listed: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        let reported = new.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            listed.contains(&reported),
            "the rename reported {reported:?}, which the directory does not hold: {listed:?}"
        );
    }

    /// `stored_spelling`'s identity branch — the half that only runs when the name
    /// asked for and the name on disk differ.
    ///
    /// The cause we actually care about is HFS+ decomposing to NFD, which no seat can
    /// stage on ext4 or NTFS. But the *mechanism* is "a lookup that succeeds under a
    /// spelling the directory does not hold", and a case-insensitive filesystem
    /// reproduces that exactly: on NTFS and APFS, `NOTES.MD` opens the file stored as
    /// `notes.md`. So this drives the identical code path through the identical GIO
    /// calls, and pins the branch that would otherwise be reachable on no CI machine
    /// at all.
    ///
    /// **That worked, and it is worth recording that it worked rather than that it
    /// was clever.** On the mac seat's default case-insensitive APFS this test does
    /// not skip — it runs, and passes. So the branch is unexecuted on ext4 and
    /// executed on macOS, which is exactly what substituting the mechanism for the
    /// unreachable cause was for.
    ///
    /// Case-sensitivity is probed at runtime rather than assumed from the platform:
    /// APFS can be formatted either way and so can ext4's `casefold`, so `cfg!` would
    /// be answering a different question than the one that matters. Skipping is
    /// announced, never silent — a vacuous pass that reads as coverage is worse than
    /// an absence anyone can look up.
    #[test]
    fn a_name_the_directory_does_not_hold_resolves_to_the_one_it_does() {
        let dir = tempfile::tempdir().unwrap();
        let stored = dir.path().join("notes.md");
        std::fs::write(&stored, "# body\n").unwrap();

        let shouted = dir.path().join("NOTES.MD");
        if !shouted.exists() {
            // Through the shared helper, not a local `eprintln!` — the helper is the
            // only thing that emits the line atomically (ScrAP-273), and reaching for
            // `eprintln!` here is exactly the miss its own module header predicts: a
            // remedy that lives behind a symlink-shaped name is one the next
            // differently-shaped test will not find.
            crate::testsymlink::skipped(
                "TDD 24.13 stored spelling",
                "the temp filesystem is case-sensitive, so no spelling mismatch can be \
                 staged on it — `stored_spelling`'s identity branch is unproven here",
            );
            return;
        }

        let spelling = stored_spelling(&gtk::gio::File::for_path(&shouted), "NOTES.MD");
        assert_eq!(
            spelling.as_deref(),
            Some("notes.md"),
            "a lookup under a spelling the directory does not hold must resolve to the \
             spelling it does — this is the NFD case's mechanism"
        );
    }

    /// The fast path, asserted as its own fact: when the directory *does* hold the
    /// requested spelling, nothing is substituted for it. Without this, a bug that
    /// returned some other entry's name would still satisfy the test above.
    #[test]
    fn a_name_the_directory_does_hold_is_left_exactly_as_asked() {
        let dir = tempfile::tempdir().unwrap();
        let doc = dir.path().join("notes.md");
        std::fs::write(&doc, "# body\n").unwrap();
        // A second entry, so "returns None" cannot be an artefact of there being
        // nothing else the identity branch could have picked.
        std::fs::write(dir.path().join("other.md"), "# other\n").unwrap();

        assert_eq!(
            stored_spelling(&gtk::gio::File::for_path(&doc), "notes.md"),
            None,
            "the requested spelling is the authoritative one when the directory holds it"
        );
    }

    /// A hard link is a *second name for the same file*, so it carries the same
    /// `id::file` as the document — and identity is the only thing the scan above
    /// matches on. The requested spelling must still win, whichever of the two the
    /// directory happens to hand back first.
    ///
    /// `readdir` order is unspecified (ext4 hashes it, so it is neither creation nor
    /// alphabetical order), which is precisely why this cannot be left to whichever
    /// entry is seen first: the aliases below are enough that some ordering puts one
    /// of them ahead of `notes.md`, and a scan that answered from the first identity
    /// match would then rename the tab onto the alias's name. Deciding only after the
    /// whole directory has been read makes the answer independent of that order.
    #[test]
    fn a_hard_link_beside_the_document_does_not_steal_its_name() {
        let dir = tempfile::tempdir().unwrap();
        let doc = dir.path().join("notes.md");
        std::fs::write(&doc, "# body\n").unwrap();
        for alias in ["alias.md", "bnotes.md", "znotes.md", "0notes.md"] {
            std::fs::hard_link(&doc, dir.path().join(alias)).unwrap();
        }

        assert_eq!(
            stored_spelling(&gtk::gio::File::for_path(&doc), "notes.md"),
            None,
            "the directory holds `notes.md`, so that is the name — an alias sharing \
             its inode is not a spelling correction"
        );
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
