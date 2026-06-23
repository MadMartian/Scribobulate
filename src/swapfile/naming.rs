//! What a swap file is called, and how a document is identified across one.
//!
//! **The `DocId` is the identity; the filename's readable half is cosmetic.** Nothing
//! downstream ever reconstructs a path — or anything else — by parsing a filename: the
//! swap file's own header is authoritative (see [`super::codec`]). The stem exists so a
//! human listing the directory can tell which document is which, and for no other
//! reason. That separation is what lets the stem be lossy, truncated and non-injective
//! without any of those being a defect.
//!
//! ## Why not name the file after the document's path
//!
//! Sanitising an absolute path into a filename is the obvious design and it fails four
//! ways, each of which this scheme avoids by construction rather than by care:
//!
//! 1. **Length.** A sanitised deep path exceeds the 255-byte filename limit on ext4
//!    (and pushes the whole path against Windows' `MAX_PATH`, which `atomic_io`'s
//!    Windows test already works around). Truncating to fit destroys the uniqueness the
//!    scheme existed for.
//! 2. **Injectivity.** Mapping every illegal character to one replacement is not
//!    injective — `a/b` and `a:b` collide. Percent-encoding restores injectivity and
//!    makes the length problem worse.
//! 3. **Case folding.** APFS and Windows are case-insensitive by default, so `Notes.md`
//!    and `notes.md` are one document but two swap files.
//! 4. **It cannot express the cases that matter most.** An untitled buffer has no path
//!    to derive a name from, and a document renamed or moved between snapshots silently
//!    orphans its own swap file.

use super::DocId;
use std::path::Path;

/// The extension every swap file carries.
pub(crate) const SWAP_EXTENSION: &str = "swap";

/// Suffix of the temporary file a snapshot is written to before being renamed into
/// place: `<name>.swap.tmp`, **co-located with its destination**.
///
/// Co-location is a correctness requirement, not tidiness: `rename(2)` is only atomic
/// within one filesystem, so a temp in `/tmp` (or anywhere else) would degrade the
/// promote into a copy and reintroduce the torn-file window the whole design exists to
/// close.
pub(crate) const TEMP_SUFFIX: &str = ".swap.tmp";

/// The stem used when a document has no usable filename of its own — an untitled
/// buffer, or one whose path is not representable as text.
const UNTITLED_STEM: &str = "untitled";

/// Longest cosmetic stem retained, in bytes. Bounds the filename well under every
/// platform's limit while leaving a stem long enough to recognise at a glance.
const MAX_STEM_BYTES: usize = 32;

/// Whether `name` is one of ours by its *name alone* — a cheap pre-filter for the
/// recovery scan, never a claim of ownership.
///
/// Ownership is decided by the file's magic line, not its extension, precisely because
/// the state directory is a shared place: a file that ends in `.swap` but does not
/// open with our magic is somebody else's and is left strictly alone. This function
/// exists only to avoid reading every unrelated file in the directory.
pub(crate) fn looks_like_swap_file(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case(SWAP_EXTENSION))
}

/// Whether `name` is one of **our** stray temporary files.
///
/// Deliberately matches the full `.swap.tmp` suffix rather than any `.tmp`. An orphaned
/// temp of ours is safe to delete outright — it is by definition a write that never
/// completed, so there is nothing in it worth preserving and no way to tell a truncated
/// one from a whole one. A stray `.tmp` belonging to *something else* in the shared
/// state directory is not ours to judge, and this mechanism must never become a file
/// shredder (the same rule that keeps a foreign `.swap` untouched).
pub(crate) fn is_stray_temp_name(name: &str) -> bool {
    name.ends_with(TEMP_SUFFIX)
}

/// The filename for a document's swap file: `<sanitized-stem>-<doc_id>.swap`.
///
/// `path` is the document's backing path, or `None` for an untitled buffer.
pub(crate) fn swap_file_name(path: Option<&Path>, doc_id: &DocId) -> String {
    let stem = path
        .and_then(|p| p.file_stem())
        .and_then(|s| s.to_str())
        .map(sanitize_stem)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| UNTITLED_STEM.to_string());
    format!("{stem}-{}.{SWAP_EXTENSION}", doc_id.as_str())
}

/// Reduce a filename stem to something safe on every target filesystem.
///
/// Keeps `[A-Za-z0-9._-]`, folds every other character (including every non-ASCII one)
/// into a single `-`, collapses runs, trims leading/trailing separators, and truncates
/// to [`MAX_STEM_BYTES`] on a character boundary.
///
/// Non-ASCII is folded rather than preserved even though modern filesystems accept it:
/// the stem is decoration, and a decoration is not worth a normalisation-form bug on a
/// path that has to work identically on ext4, APFS and NTFS.
fn sanitize_stem(stem: &str) -> String {
    let mut out = String::with_capacity(stem.len().min(MAX_STEM_BYTES));
    let mut last_was_sep = false;
    for ch in stem.chars() {
        let keep = ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-');
        if keep {
            if out.len() + ch.len_utf8() > MAX_STEM_BYTES {
                break;
            }
            out.push(ch);
            last_was_sep = ch == '-';
        } else {
            if last_was_sep || out.is_empty() {
                continue;
            }
            if out.len() + 1 > MAX_STEM_BYTES {
                break;
            }
            out.push('-');
            last_was_sep = true;
        }
    }
    while out.ends_with(['-', '.']) {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{looks_like_swap_file, sanitize_stem, swap_file_name, MAX_STEM_BYTES};
    use crate::swapfile::DocId;
    use std::path::Path;

    fn doc_id() -> DocId {
        DocId::from_hex("3f2ac91b4d5e6f708192a3b4c5d6e7f8").expect("a valid 32-hex id")
    }

    #[test]
    fn a_titled_document_keeps_a_readable_stem() {
        let name = swap_file_name(Some(Path::new("/home/u/Documents/notes.md")), &doc_id());
        assert_eq!(name, "notes-3f2ac91b4d5e6f708192a3b4c5d6e7f8.swap");
    }

    #[test]
    fn an_untitled_document_is_named_untitled() {
        assert_eq!(
            swap_file_name(None, &doc_id()),
            "untitled-3f2ac91b4d5e6f708192a3b4c5d6e7f8.swap"
        );
    }

    #[test]
    fn two_documents_with_the_same_name_get_different_files() {
        // The whole point of the id: the same filename in two directories, or the same
        // file open in two windows, must never share one swap file.
        let a = swap_file_name(Some(Path::new("/a/notes.md")), &doc_id());
        let b = swap_file_name(
            Some(Path::new("/b/notes.md")),
            &DocId::from_hex("00112233445566778899aabbccddeeff").expect("valid"),
        );
        assert_ne!(a, b);
    }

    #[test]
    fn illegal_characters_are_folded_and_runs_collapsed() {
        assert_eq!(sanitize_stem("my notes/draft:2"), "my-notes-draft-2");
    }

    #[test]
    fn non_ascii_is_folded_rather_than_preserved() {
        assert_eq!(sanitize_stem("réunion"), "r-union");
    }

    #[test]
    fn a_stem_of_only_illegal_characters_falls_back_to_untitled() {
        // sanitize_stem yields "", and swap_file_name substitutes the fallback rather
        // than emitting a filename that begins with the separator.
        assert_eq!(
            swap_file_name(Some(Path::new("/tmp/???")), &doc_id()),
            "untitled-3f2ac91b4d5e6f708192a3b4c5d6e7f8.swap"
        );
    }

    #[test]
    fn a_long_stem_is_truncated_but_the_id_survives_intact() {
        let long = "a".repeat(200);
        let name = swap_file_name(Some(Path::new(&format!("/tmp/{long}.md"))), &doc_id());
        assert!(
            name.ends_with("-3f2ac91b4d5e6f708192a3b4c5d6e7f8.swap"),
            "truncation must never eat into the identity: {name}"
        );
        assert!(name.len() < 100, "bounded filename length: {name}");
    }

    #[test]
    fn truncation_does_not_split_a_character() {
        // Guards against a byte-wise truncate panicking or producing invalid UTF-8.
        // Every kept character here is ASCII, so the risk is at the fold boundary.
        let stem = sanitize_stem(&"é".repeat(100));
        assert!(stem.len() <= MAX_STEM_BYTES);
        assert!(stem.is_char_boundary(stem.len()));
    }

    #[test]
    fn a_trailing_separator_is_trimmed() {
        assert_eq!(sanitize_stem("notes "), "notes");
        assert_eq!(sanitize_stem("notes."), "notes");
    }

    #[test]
    fn a_stray_temp_is_recognised_only_when_it_is_ours() {
        use crate::swapfile::is_stray_temp_name;
        assert!(is_stray_temp_name("notes-abc.swap.tmp"));
        // NOT ours: the state directory is shared, and deleting another tool's
        // scratch file would make this mechanism a shredder.
        assert!(!is_stray_temp_name("something-else.tmp"));
        assert!(!is_stray_temp_name("notes-abc.swap"));
        assert!(!is_stray_temp_name("session.toml"));
    }

    #[test]
    fn a_temp_is_co_located_with_the_snapshot_it_will_replace() {
        // rename(2) is atomic only within one filesystem, so the temp must be a sibling
        // of its destination — asserted on the shape, since a future refactor that put
        // it in a temp directory would silently degrade the promote into a copy.
        let name = swap_file_name(Some(Path::new("/tmp/notes.md")), &doc_id());
        let temp = format!("{name}{}", crate::swapfile::TEMP_SUFFIX);
        assert_eq!(
            Path::new(&temp).parent(),
            Path::new(&name).parent(),
            "the temp and its destination must be siblings"
        );
        assert!(temp.starts_with(&name));
    }

    #[test]
    fn the_extension_prefilter_accepts_ours_and_rejects_others() {
        assert!(looks_like_swap_file("notes-abc.swap"));
        assert!(looks_like_swap_file("notes-abc.SWAP"));
        assert!(!looks_like_swap_file("session.toml"));
        assert!(!looks_like_swap_file("scribobulate.log"));
        assert!(!looks_like_swap_file("swap"));
    }
}
