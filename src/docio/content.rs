//! The screen a document's **bytes** pass before any of them reach the editor, the
//! screen its **name** passes before anything touches it ([`screen_name`], Windows
//! device names), and the one refusal type every reader in [`super`] reports.
//!
//! [`crate::limits`] admits a path from its metadata alone — a regular file, within the
//! size cap — and that is all a path can tell you before it is read. Some refusals can
//! only be made once the bytes are in hand, and this module owns those.
//!
//! # The one it holds: a NUL byte (TDD 1.4a, 5.5)
//!
//! A NUL is valid UTF-8, so `read_to_string` accepts a file that carries one, while a
//! `GtkTextBuffer` cannot hold one: `gtk_text_buffer_emit_insert` asserts
//! `g_utf8_validate (text, len, NULL)`, and GLib's validator rejects a NUL inside the
//! given length. `set_text` inserts the whole document in ONE call, so the assertion
//! drops **all of it**, not just the offending paragraph — the editor comes up empty.
//!
//! That is data loss, not a display fault, because the rest of the tab believed the
//! load: the saved baseline and the derived views' source were set to the full text,
//! and "dirty" is defined as `editor_text() != saved_baseline`. An empty buffer against
//! a full baseline is a tab that opens already dirty, with Save enabled, and a Save with
//! nothing typed writes the empty buffer over the file and reports success. MEASURED on
//! Linux and Windows (manual 4.2-c).
//!
//! Invalid UTF-8 never had this hole — `read_to_string` fails, and the tab shows a
//! notice with no backing path — so the fix is to give NUL the same shape: refused at
//! the module's door, with a notice naming why, and no path a save could write to.
//! Every reader applies it, for the reasons [`super`] gives for the BOM strip: the save
//! guard and crash recovery compare against what these readers return, and a reader
//! that admitted what another refused would make those comparisons disagree.

use super::rename::FsRules;
use crate::limits;
use std::path::Path;

/// Why a document's bytes were refused once read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ContentRefusal {
    /// The file holds a NUL at this byte offset (counted from the start of the file,
    /// a byte-order mark included), so it is binary data, not text the editor can take.
    ContainsNul { offset: usize },
}

impl std::fmt::Display for ContentRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContentRefusal::ContainsNul { offset } => write!(
                f,
                "contains a NUL byte (at byte {offset}), so it is binary data rather \
                 than a text document"
            ),
        }
    }
}

/// Why a document was refused: from its name or its metadata, before the read, or from
/// its bytes, after it. One type so the notice and the log are worded by one `Display`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// Windows: the path's filename names a device (`nul.md` is the NUL device), so
    /// opening it reaches the device, not a file ([`screen_name`], TDD 1.4a).
    /// Carries the device's canonical name, never the filename, so the notice cannot
    /// echo a hostile name outside the code span `refusal_doc` builds for it.
    DeviceName {
        device: &'static str,
    },
    Admission(limits::LoadRefusal),
    Content(ContentRefusal),
}

impl From<limits::LoadRefusal> for Refusal {
    fn from(r: limits::LoadRefusal) -> Self {
        Refusal::Admission(r)
    }
}

impl From<ContentRefusal> for Refusal {
    fn from(r: ContentRefusal) -> Self {
        Refusal::Content(r)
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::DeviceName { device } => write!(
                f,
                "{device} is a Windows device name, not a file: Windows reserves it under \
                 any extension, so this path would reach the device instead"
            ),
            Refusal::Admission(r) => r.fmt(f),
            Refusal::Content(r) => r.fmt(f),
        }
    }
}

impl Refusal {
    /// The refusal as the `io::Error` the comparison readers report, so each caller's
    /// existing read-failure handling covers it (Reload reports it, the save guard asks
    /// before overwriting, the watcher stays silent).
    pub(crate) fn into_io_error(self) -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::InvalidData, self.to_string())
    }
}

/// The name half of admission, asked before anything touches the path (TDD 1.4a).
///
/// On Windows a filename can name a DEVICE rather than a file — `nul.md` is the NUL
/// device, with any extension, in any case, trailing dots and spaces included
/// ([`super::rename::windows_device_name`]). Nothing on disk says so, and asking the
/// disk opens the device. MEASURED on Windows 10 before this screen:
/// `metadata("nul.md")` fails with "Incorrect function." (os error 1), which the
/// metadata admission deliberately does not treat as a refusal, and `read_to_string`
/// then succeeds with nothing — so `nul.md` opened as an empty document with a backing
/// path ("loaded … (0 bytes)"). `com9.md`, with no such port, read as `NotFound`: a NEW
/// document whose save would address the port. So the refusal is made from the name,
/// first, by every reader in [`super`] and, for every writer, by `atomic_io`'s staging
/// step — both export sinks and the session file, not only the document save.
///
/// The one screen here that needs no bytes; it lives beside [`screen`] because it
/// reports through the same [`Refusal`]. The rules are a parameter for the reason
/// `rename`'s module doc gives — a pure function over plain data is checkable for every
/// platform from any host — and the callers pass `host_rules()`, so on Linux and macOS
/// `nul.md` stays an ordinary name.
pub(crate) fn screen_name(path: &Path, rules: FsRules) -> Result<(), Refusal> {
    let Some(name) = path.file_name() else {
        return Ok(());
    };
    match rules.device_name(&name.to_string_lossy()) {
        Some(device) => Err(Refusal::DeviceName { device }),
        None => Ok(()),
    }
}

/// The filesystem's own answer to whether `path`'s name is a device on this host — the
/// oracle the device tests decide by (ISSUES BT). It writes `contents` to `path`
/// UNSCREENED and reports whether `path`'s directory, which must start empty, then lists
/// anything: a device swallows the write and leaves nothing.
///
/// No name rule decides what the tests expect, because which names are devices depends
/// on the Windows release: Windows 10 reserves `nul.md`, newer releases reportedly do
/// not, and on Linux and macOS no name is a device. Only names whose device swallows a
/// write harmlessly belong here — `NUL`, and a `COM`/`LPT` port the host does not have,
/// whose open fails — never `CON`, `PRN` or `AUX`.
#[cfg(test)]
pub(crate) fn host_makes_a_file_of(path: &Path, contents: &str) -> bool {
    let dir = path.parent().expect("a path in a directory");
    assert_eq!(
        std::fs::read_dir(dir).expect("list the directory").count(),
        0,
        "the oracle needs an empty directory"
    );
    let _ = std::fs::write(path, contents);
    std::fs::read_dir(dir).expect("list the directory").count() > 0
}

/// Screen a document's raw bytes, BEFORE any byte-order mark is stripped, so the offset
/// reported is the one a hex dump of the file shows.
pub(crate) fn screen(bytes: &[u8]) -> Result<(), ContentRefusal> {
    match bytes.iter().position(|&b| b == 0) {
        Some(offset) => Err(ContentRefusal::ContainsNul { offset }),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nul_anywhere_is_refused_at_its_file_offset() {
        assert_eq!(
            screen(b"# Title\n\nbefore\x00after\n"),
            Err(ContentRefusal::ContainsNul { offset: 15 })
        );
        assert_eq!(
            screen(b"\x00"),
            Err(ContentRefusal::ContainsNul { offset: 0 })
        );
        assert_eq!(
            screen(b"text\x00"),
            Err(ContentRefusal::ContainsNul { offset: 4 })
        );
    }

    /// The common case, and the neighbours a careless screen would sweep up: other
    /// control characters, a BOM, multi-byte UTF-8 and an empty file are all text.
    #[test]
    fn text_without_a_nul_passes_including_other_control_characters() {
        for ok in [
            &b""[..],
            b"# Title\n",
            b"\xEF\xBB\xBF# bom\n",
            b"tab\tform\x0cfeed\x01soh\x7fdel\r\n",
            "caf\u{e9} \u{1F600}\n".as_bytes(),
        ] {
            assert_eq!(screen(ok), Ok(()), "{ok:?}");
        }
    }

    /// **The name screen is Windows' rule, applied only under Windows' rules (TDD 1.4a).**
    /// Pure, so both halves are asserted on every host: a device name is refused under
    /// [`FsRules::Windows`] with the device named, and is an ordinary name under POSIX.
    /// Only the last component counts — a folder called `nul` is not a device.
    #[test]
    fn the_name_screen_refuses_a_device_only_under_windows_rules() {
        let dir = Path::new("docs").join("sub");
        for (name, device) in [
            ("nul.md", "NUL"),
            ("Com¹.md", "COM¹"),
            ("conout$.md.", "CONOUT$"),
        ] {
            let path = dir.join(name);
            assert_eq!(
                screen_name(&path, FsRules::Windows),
                Err(Refusal::DeviceName { device }),
                "{name}"
            );
            assert_eq!(screen_name(&path, FsRules::Posix), Ok(()), "{name}");
        }
        let in_a_nul_folder = Path::new("nul").join("notes.md");
        assert_eq!(screen_name(&in_a_nul_folder, FsRules::Windows), Ok(()));
        let nulbyte = dir.join("nulbyte.md");
        assert_eq!(screen_name(&nulbyte, FsRules::Windows), Ok(()));
        let why = Refusal::DeviceName { device: "NUL" }.to_string();
        assert!(why.contains("NUL is a Windows device name"), "{why}");
    }

    /// The notice names the reason in words a reader can act on, and the offset.
    #[test]
    fn the_refusal_says_why_and_where() {
        let why = Refusal::from(ContentRefusal::ContainsNul { offset: 15 }).to_string();
        assert!(why.contains("NUL"), "{why}");
        assert!(why.contains("15"), "{why}");
        assert!(why.contains("binary"), "{why}");
        let io = Refusal::from(ContentRefusal::ContainsNul { offset: 0 }).into_io_error();
        assert_eq!(io.kind(), std::io::ErrorKind::InvalidData);
        assert!(io.to_string().contains("NUL"));
    }
}
