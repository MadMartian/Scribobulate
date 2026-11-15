//! Which document paths GIO's Win32 file monitor cannot watch at all — so live reload
//! there has to come from somewhere else (`window::rewatch`).
//!
//! **GIO opens the watched file's folder as `\\?\` followed by the folder's path**
//! (`gwin32fsmonitorutils.c`, `LONGPFX`), whatever form that path is in. For a drive path
//! that is the long-path form of the same folder. For a UNC path it is not: `\\server\share\dir`
//! becomes `\\?\\\server\share\dir`, which `CreateFileW` refuses with `ERROR_INVALID_NAME`
//! (123) — the verbatim spelling of a share is `\\?\UNC\server\share\dir`. GIO checks that
//! open only to decide whether to start `ReadDirectoryChangesW`; it never reports the
//! failure, so `g_file_monitor_file` hands back a live monitor that never fires. The
//! verbatim spelling cannot be fed in instead: `GLocalFile` drops a leading `\\?\` and,
//! finding `UNC\server\…` relative, resolves it against the working directory, so the
//! monitor would watch a folder that does not exist. MEASURED on GLib 2.90.1 (Windows seat,
//! 2026-11-14), the same file written through its drive path every 100 ms for 3 s:
//!
//! | the monitor is attached on | `GFile`'s path | events |
//! |---|---|---|
//! | `C:\…\watched.md` | as given | `Changed` ×4 |
//! | `\\?\C:\…\watched.md` | `C:\…` (prefix dropped) | `Changed` ×4 |
//! | `\\localhost\C$\…\watched.md` | as given | **none** |
//! | `\\?\UNC\localhost\C$\…\watched.md` | `<cwd>\UNC\localhost\C$\…` | **none** |
//!
//! and `CreateFileW(FILE_LIST_DIRECTORY, FILE_FLAG_BACKUP_SEMANTICS)` on the folder: `\\?\C:\…`,
//! `\\?\UNC\localhost\C$\…` and `\\localhost\C$\…` open; `\\?\\\localhost\C$\…` — the string
//! GIO builds — fails with 123. A .NET `FileSystemWatcher` on the same share does see the
//! writes, so the share itself reports changes; it is only GIO's spelling of it that fails.
//!
//! The other non-disk prefixes — `\\?\Volume{…}\` and the `\\.\` device namespace — were
//! not measured. They are answered blind too, from the same code: the first loses its
//! prefix and turns relative as `\\?\UNC\` does, the second gains `\\?\` in front of `\\.`.
//! Calling a watchable path blind costs one metadata read per poll; calling a blind one
//! watchable is the defect, so the doubt resolves this way.

use std::path::{Component, Path, Prefix};

/// Whether GIO's file monitor, attached on `path`, would be a monitor that never fires.
///
/// True for a path that names its folder through a share or a verbatim non-disk prefix;
/// false for a drive path in either spelling, and for anything without a prefix (which no
/// open document has: its path is absolute).
pub(crate) fn doc_monitor_is_blind_at(path: &Path) -> bool {
    match path.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(_) | Prefix::VerbatimDisk(_) => false,
            Prefix::UNC(..)
            | Prefix::VerbatimUNC(..)
            | Prefix::Verbatim(_)
            | Prefix::DeviceNS(_) => true,
        },
        _ => false,
    }
}
