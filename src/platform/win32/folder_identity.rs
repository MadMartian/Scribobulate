//! Which directory stands at a path — the source live reload needs to notice its folder
//! being replaced, because GIO's Win32 file monitor cannot.
//!
//! **GIO's Win32 monitor is bound to a directory, not to a path.** It opens a handle on
//! the watched file's folder and calls `ReadDirectoryChangesW` on that handle, so when
//! the folder is deleted, or moved away (Explorer's delete is a move into the Recycle
//! Bin), the handle goes on naming the old directory and the monitor never fires again —
//! not for a folder recreated at the same path, not for the file written into it. A
//! monitor attached while the folder is missing is returned as a live monitor and never
//! fires either. MEASURED on GLib 2.90.1 (Windows seat, 2026-11-14), one probe per shape:
//!
//! | shape | events after the shape | after recreate + write | after a later append |
//! |---|---|---|---|
//! | `remove_dir_all`, wait, recreate | `Changed`, `ChangesDoneHint`, `Deleted` | none | none |
//! | `remove_dir_all` + recreate in one burst | one `Deleted` | none | none |
//! | folder renamed away, recreated | **none at all** | none | none |
//! | a monitor attached while the folder is missing | — | none | none |
//! | a fresh monitor attached after the recreate | — | — | `Changed` |
//!
//! Linux keeps firing after a folder is deleted and recreated (the Linux seat observed it):
//! GIO's inotify backend keeps the watches whose directory vanished on a list it retries
//! (`inotify-missing.c` — read, not measured here), and the Win32 backend has no such
//! list. That is why the defect is Windows-only. The
//! repair — re-attach once a different folder stands at the path — is the application's
//! and shared (`window::rewatch`); this module only answers *which* directory is there,
//! by its file identity, so the shared code can tell a replaced folder from the same
//! one.

use std::ffi::c_void;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;

use crate::platform::FolderIdentity;

#[link(name = "kernel32")]
extern "system" {
    fn GetFileInformationByHandle(file: *mut c_void, info: *mut ByHandleFileInformation) -> i32;
}

/// `BY_HANDLE_FILE_INFORMATION`. Only the volume serial and the file index are read; the
/// rest is laid out so the struct is the size the call writes.
#[repr(C)]
#[derive(Default)]
struct ByHandleFileInformation {
    file_attributes: u32,
    creation_time: [u32; 2],
    last_access_time: [u32; 2],
    last_write_time: [u32; 2],
    volume_serial_number: u32,
    file_size_high: u32,
    file_size_low: u32,
    number_of_links: u32,
    file_index_high: u32,
    file_index_low: u32,
}

/// `FILE_FLAG_BACKUP_SEMANTICS` — required for `CreateFileW` to open a directory at all.
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
/// `FILE_READ_ATTRIBUTES` — all the identity query needs; no read of the directory.
const FILE_READ_ATTRIBUTES: u32 = 0x0080;

/// The identity of the directory at `dir`: its volume serial number and file index.
///
/// Opened with `std`'s default share mode, which includes `FILE_SHARE_DELETE`, so the
/// probe never stops anyone deleting, renaming or recreating the folder it is asking
/// about. A missing folder is `NotFound` (`ERROR_FILE_NOT_FOUND` and
/// `ERROR_PATH_NOT_FOUND` both map there), which is how the caller tells it from a
/// folder it merely could not ask about.
pub(crate) fn folder_identity(dir: &Path) -> std::io::Result<FolderIdentity> {
    let handle = std::fs::OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(dir)?;
    let mut info = ByHandleFileInformation::default();
    // SAFETY: `handle` is an open file handle owned by `handle` for the whole call, and
    // `info` is a live, correctly sized `BY_HANDLE_FILE_INFORMATION` the call writes into.
    let ok =
        unsafe { GetFileInformationByHandle(handle.as_raw_handle(), std::ptr::addr_of_mut!(info)) };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let index = (u64::from(info.file_index_high) << 32) | u64::from(info.file_index_low);
    Ok(FolderIdentity {
        volume: u64::from(info.volume_serial_number),
        index: u128::from(index),
    })
}
