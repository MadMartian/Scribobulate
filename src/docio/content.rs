//! The screen a document's **bytes** pass before any of them reach the editor, and the
//! one refusal type every reader in [`super`] reports.
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

use crate::limits;

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

/// Why a document was refused: from its metadata, before the read, or from its bytes,
/// after it. One type so the notice and the log are worded by one `Display`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Refusal {
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
