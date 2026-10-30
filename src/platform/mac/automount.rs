//! Whether a directory belongs to the automounter, read with `statfs`.
//!
//! macOS's automounter (`automountd`, configured by `/etc/auto_master`) serves each map
//! as an `autofs` filesystem. Looking up a name inside one mounts what the name denotes:
//! under `/net -hosts`, `/net/<host>` asks `<host>` for its NFS exports. A plain
//! `lstat` of that name is enough to trigger it. MEASURED on macOS 27 with `/net -hosts`
//! enabled: `symlink_metadata("/net/203.0.113.1")` stalls 6 s and fails "Operation timed
//! out"; a repeat for the same host is answered from a negative cache.
//!
//! `statfs` on the map's own directory does not trigger anything: it reports the
//! directory's filesystem, `autofs`, instantly. MEASURED the same way: `statfs` on
//! `/net` and `/System/Volumes/Data/net` returned `autofs` in under a millisecond, and a
//! fresh host's `lstat` afterwards still took the full 6 s, so nothing was mounted.
//!
//! Supplies the fact only. What to do about it is the image and link gates' decision
//! (`links::links_reach_foreign`).

use std::ffi::{CStr, CString};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// Whether `dir` is on an `autofs` filesystem, so that looking up any name inside it
/// can mount something. `false` when `dir` cannot be examined: `statfs` fails only where
/// `dir` itself cannot be reached (gone, or a search permission missing on the way), and
/// then the `lstat` of a name inside it, the lookup this guards, fails the same way
/// without reaching the automounter. The cross-platform contract, and why this reads
/// through the path rather than from the mount table, is `platform::mounts_on_lookup`'s.
pub(crate) fn is_automount_directory(dir: &Path) -> bool {
    let Ok(path) = CString::new(dir.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `statfs` is plain data, so an all-zero value is a valid instance for the
    // call to overwrite.
    let mut info: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: `path` is a NUL-terminated string that outlives the call, and `info` is a
    // valid, writable `statfs` for the call to fill.
    if unsafe { libc::statfs(path.as_ptr(), &mut info) } != 0 {
        return false;
    }
    // SAFETY: on success the kernel fills `f_fstypename` with a NUL-terminated name no
    // longer than the array (MFSTYPENAMELEN), so the read stays inside it.
    let fstype = unsafe { CStr::from_ptr(info.f_fstypename.as_ptr()) };
    fstype.to_bytes() == b"autofs"
}

#[cfg(test)]
mod tests {
    use super::is_automount_directory;
    use std::path::Path;

    #[test]
    fn an_ordinary_directory_is_not_an_automount() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_automount_directory(dir.path()));
        assert!(!is_automount_directory(Path::new("/")));
    }

    #[test]
    fn a_missing_directory_is_not_an_automount() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_automount_directory(&dir.path().join("absent")));
    }

    /// Stock macOS mounts `/home` through the `auto_home` map, so its directory is the
    /// one automounted directory every host should have, `/net` being off by default.
    #[test]
    fn the_stock_home_map_is_an_automount() {
        let home = Path::new("/System/Volumes/Data/home");
        if !home.is_dir() {
            println!("SKIPPED [TDD 2.7]: this host has no auto_home map at {home:?}");
            return;
        }
        assert!(is_automount_directory(home));
    }
}
