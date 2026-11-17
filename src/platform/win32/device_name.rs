//! Which filenames Windows treats as a device — asked of Windows, because the answer
//! depends on the Windows release and no name rule can know which one it is running on.
//!
//! **The cause.** On Windows 10 a filename whose part before the first dot is a device name
//! (`nul.md`, `COM1.txt`) opens the device: a read returns nothing and a write disappears.
//! Windows 11 and Windows Server 2025 reportedly stopped treating a device name WITH an
//! extension as a device there, so the same name can be an ordinary file. The application's
//! name rule (`docio::rename::windows_device_name`) is the Windows 10 answer, and applied
//! on a newer host it refuses a real file (ISSUES BT).
//!
//! **The question asked.** `GetFullPathNameW` is the conversion every Win32 file call makes
//! on a DOS path before it opens anything, and it rewrites a name it treats as a device to
//! the device namespace, `\\.\<device>`. It reads nothing from disk, so asking it costs no
//! I/O and cannot touch the device. MEASURED on Windows 10 (19045, 2026-11-16 and
//! 2026-11-17): `nul.md`, `NUL`, `nul .md`, `nul.tar.gz`, `con.md`, `COM1.md`, `com9.md.`,
//! `lpt1.txt`, `aux`, `prn.md`, `conin$`, `CONOUT$.md`, `COM¹.md`, `nul..md`, `aux.md.` map
//! to `\\.\…`; `COM0.md`, `com10.md`, `nulbyte.md`, ` nul.md`, `nul\t.md`, `clock$.md` and
//! `COM⁴.md` stay ordinary paths — exactly the name rule's verdict on every one of them.
//!
//! Only a bare name is asked about. A name carrying a separator is not a filename, and
//! `GetFullPathNameW` would answer for its last component instead (`x\nul` → `\\.\nul`).

use super::wide;

#[link(name = "kernel32")]
extern "system" {
    fn GetFullPathNameW(
        name: *const u16,
        buffer_len: u32,
        buffer: *mut u16,
        file_part: *mut *mut u16,
    ) -> u32;
}

/// The device namespace prefix `GetFullPathNameW` rewrites a device name into.
const DEVICE_PREFIX: &str = r"\\.\";

/// Whether Windows treats the filename `name` as a device: `Some(Some(device))` with the
/// device as Windows spelled it after `\\.\`, `Some(None)` for an ordinary filename, and
/// `None` when Windows could not be asked — the call failed, or `name` is not a bare
/// filename (empty, or carrying a separator or a NUL).
pub(crate) fn device_behind_name(name: &str) -> Option<Option<String>> {
    if name.is_empty() || name.contains(['\\', '/', '\0']) {
        return None;
    }
    full_path_name(name).map(|full| full.strip_prefix(DEVICE_PREFIX).map(str::to_owned))
}

/// `GetFullPathNameW(name)`, or `None` if the call failed.
fn full_path_name(name: &str) -> Option<String> {
    let name = wide(name);
    let mut buffer = vec![0u16; 260];
    // Two passes at most: a too-small buffer is answered with the size it needs, which
    // the second call is given. A third answer that still does not fit means the working
    // directory changed in between; that is a failure to ask, not an answer.
    for _ in 0..2 {
        let len = u32::try_from(buffer.len()).ok()?;
        // SAFETY: `name` is NUL-terminated UTF-16 (`wide`); `buffer` is writable for `len`
        // units; a null `file_part` is documented as allowed.
        let written = unsafe {
            GetFullPathNameW(
                name.as_ptr(),
                len,
                buffer.as_mut_ptr(),
                std::ptr::null_mut(),
            )
        };
        let written = usize::try_from(written).ok()?;
        if written == 0 {
            return None;
        }
        if written < buffer.len() {
            return String::from_utf16(&buffer[..written]).ok();
        }
        buffer.resize(written, 0);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::device_behind_name;

    /// **Windows answers for the device names the rule was measured against (ISSUES BT).**
    /// The names here are device names on every Windows release, with no extension, so the
    /// assertion holds on Windows 10 and on a release that stopped reserving `nul.md`.
    #[test]
    fn a_bare_device_name_is_answered_as_that_device() {
        for (name, device) in [
            ("NUL", "NUL"),
            ("nul", "nul"),
            ("con", "con"),
            ("COM1", "COM1"),
        ] {
            assert_eq!(
                device_behind_name(name),
                Some(Some(device.to_owned())),
                "{name}"
            );
        }
    }

    /// Names that are files on every Windows release are answered as files, not as
    /// "could not ask".
    #[test]
    fn an_ordinary_name_is_answered_as_a_file() {
        for name in ["nulbyte.md", "COM0.md", "com10.md", "notes.md", " nul.md"] {
            assert_eq!(device_behind_name(name), Some(None), "{name}");
        }
    }

    /// A name that is not a bare filename is not asked about at all: `GetFullPathNameW`
    /// would answer for its last component.
    #[test]
    fn a_name_with_a_separator_or_nul_is_not_asked_about() {
        for name in ["", r"x\nul", "x/nul", "nul\0.md"] {
            assert_eq!(device_behind_name(name), None, "{name:?}");
        }
    }
}
