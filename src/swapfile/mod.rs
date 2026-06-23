//! **Swap files** — periodic full-content recovery snapshots of dirty buffers, so an
//! unclean exit no longer discards every unsaved edit.
//!
//! A *swap file* here is a **periodic full-content snapshot of a dirty buffer**,
//! rewritten on a debounce. The name is borrowed from vim, and one difference is worth
//! stating so it does not over-promise: vim's `.swp` is an incremental journal held open
//! for the whole edit session and doubling as a lock, so a second vim opening the same
//! file finds it and warns. These are neither incremental nor a lock — they are simpler,
//! never partially applied, and carry no mutual-exclusion claim.
//!
//! This module is the **display-free core**: the header codec, the naming scheme, the
//! content digest, and the recovery decisions. Everything here is a pure function over
//! plain data and is unit-tested with no display (gtk4-rs guardrail #4). The GTK and
//! filesystem edges live in `window/swap.rs` (writing, the debounce, the invariant) and
//! `window/swaprecovery.rs` (the startup pass).
//!
//! # The governing invariant
//!
//! > **A swap file exists for a document if and only if that document is dirty.**
//!
//! Every deletion rule collapses into that one statement: a save makes the document
//! clean, so its swap goes; an undo back to the on-disk content makes it clean, so its
//! swap goes; discarding an unsaved tab makes the document cease to exist, so its swap
//! goes. Implementing the invariant at the single place dirtiness is recomputed — rather
//! than teaching each of save / save-as / discard / reload / revert its own deletion
//! rule — is what makes every *future* path that changes dirtiness correct without being
//! individually taught (POLICY § "one path, not two"; ScrAP-116, ScrAP-219).
//!
//! Two properties fall out of the invariant for free, and both are load-bearing
//! elsewhere:
//!
//! - **A non-empty swap directory at startup means the last exit was unclean.** A clean
//!   quit resolves every dirty tab through Save or Discard, and both delete. So no
//!   "clean shutdown" marker file is needed — the directory's emptiness *is* the marker.
//! - **Absence of a swap always means "clean".** A session file listing a tab with no
//!   swap is never evidence that a swap was lost.
//!
//! # Self-sufficiency
//!
//! > **The swap file is self-sufficient. `session.toml` is advisory.**
//!
//! Nothing transactionally couples the swap directory to the session file: they are
//! written by different paths at different moments, and a crash can land between them,
//! so the two *will* drift. Every fact needed to recover a document — which file it
//! belongs to, whether it was untitled, what its on-disk baseline was — therefore lives
//! in the swap file's own header. The session file only helps decide *which window and
//! tab* to put the content back into, and recovery must be correct without it. That
//! principle decides the recovery algorithm's shape: **header first, session as a
//! hint** — never session-first with the header as confirmation.

pub(crate) mod codec;
pub(crate) mod digest;
pub(crate) mod naming;
pub(crate) mod recovery;

use std::path::{Path, PathBuf};

pub(crate) use codec::{decode, encode, SwapDecodeError};
pub(crate) use digest::content_digest;
pub(crate) use naming::{is_stray_temp_name, looks_like_swap_file, swap_file_name, TEMP_SUFFIX};
pub(crate) use recovery::{sync_action, SwapDisposition, SwapSync};

/// Directory under the user state directory holding every swap file.
const SWAP_DIR_NAME: &str = "swap";

/// Number of hex characters in a [`DocId`] — 128 bits.
const DOC_ID_HEX_LEN: usize = 32;

/// A document's identity for the life of its tab: 128 random bits, lowercase hex.
///
/// Allocated when a tab is created and carried unchanged through everything that would
/// otherwise break a path-derived identity — a save, a Save As, a rename on disk, a move
/// to another window. It is persisted in the session so a restored tab can be correlated
/// with its swap file, and written into the swap header, which is the authoritative copy.
///
/// **Random, not sequential.** A counter would be shorter, but two instances started
/// independently (via `--new-instance`, or on a platform with no single-instance
/// transport) would both begin at 1 and collide in a shared directory — silently, and
/// only for users who had a crash, which is the worst possible audience for a rare bug.
#[derive(Clone, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub(crate) struct DocId(String);

impl DocId {
    /// Allocate a fresh identity.
    ///
    /// Sourced from GLib's UUID generator rather than a hand-rolled RNG or a new
    /// dependency: it is already linked, it is seeded from the platform's random source,
    /// and 128 bits of it is exactly what is wanted. The dashes are stripped because the
    /// value lands in a filename, where a shorter unbroken token reads better.
    pub(crate) fn generate() -> Self {
        Self(glib::uuid_string_random().replace('-', "").to_lowercase())
    }

    /// Parse an id read back from a session file or a swap header, rejecting anything
    /// that is not exactly 32 lowercase hex characters.
    ///
    /// Validated rather than trusted because the value is interpolated into a filename:
    /// an id carrying `../` or a path separator would otherwise let a hand-edited (or
    /// corrupted) session file steer a write outside the swap directory. The swap
    /// directory is under the user's own state directory so this is a robustness
    /// boundary rather than a privilege one — but it costs one comparison.
    pub(crate) fn from_hex(text: &str) -> Option<Self> {
        let ok = text.len() == DOC_ID_HEX_LEN
            && text
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
        ok.then(|| Self(text.to_string()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// A swap file's frontmatter header: everything needed to recover the document it
/// belongs to, without consulting the session file or anything else.
///
/// Field order is the order it serialises in, and it is chosen for a human reading the
/// first few lines of a recovered file — identity, then what it is, then when.
#[derive(Clone, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct SwapHeader {
    /// Identity; correlates to a restored tab.
    pub doc_id: DocId,
    /// The twin's absolute path. `None` for an untitled buffer — and also for the rare
    /// document whose path is not representable as text, which is why `untitled` is
    /// carried separately rather than inferred from this being absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Whether the document was never saved. **Explicit, not inferred**: a header that
    /// lost its `path` must not silently become an untitled recovery, because that turns
    /// "we could not name your file" into "you never had one".
    pub untitled: bool,
    /// [`content_digest`] of the twin's content as of the last load or save. Lets
    /// recovery detect that the file changed on disk since the crash.
    pub baseline_digest: String,
    /// Unix epoch seconds. Drives the recovery notice's wording and stale-swap pruning.
    pub written_at: i64,
    /// The pid that wrote this snapshot — the liveness guard's input (see
    /// [`recovery::disposition`]).
    pub owner_pid: u32,
    /// Which build wrote it. Costs one line and is worth having in a post-crash forensic
    /// read, where the swap file may be the only artefact with a version stamp on it.
    pub app_version: String,
}

/// The swap directory, `<state dir>/swap`, or `None` where no state directory resolves.
///
/// Reached through [`crate::session::state_directory`] — the single lookup in the tree
/// for the user state directory — rather than resolving XDG again here. That is not
/// merely tidiness: it inherits the Windows/macOS fallback and the warn-once behaviour
/// that ScrAP-167 exists to preserve, both of which a second lookup would have to
/// re-derive and would eventually get wrong.
///
/// **State, not config.** TECH.md's platform notes state the rule this follows outright
/// — "configuration should roam between machines; session state should not". A swap file
/// is machine-generated, host-local and short-lived; syncing one between machines would
/// offer a recovery prompt on a machine where the crash never happened.
pub(crate) fn swap_directory() -> Option<PathBuf> {
    crate::session::state_directory().map(|dir| dir.join(SWAP_DIR_NAME))
}

/// The full path of `doc_id`'s swap file, given the document's backing path.
pub(crate) fn swap_path(path: Option<&Path>, doc_id: &DocId) -> Option<PathBuf> {
    swap_directory().map(|dir| dir.join(swap_file_name(path, doc_id)))
}

/// The temporary file a snapshot is written to before being renamed over [`swap_path`].
///
/// A sibling of its destination by construction — see [`TEMP_SUFFIX`] for why that is a
/// correctness requirement rather than a convention.
pub(crate) fn swap_temp_path(path: Option<&Path>, doc_id: &DocId) -> Option<PathBuf> {
    swap_path(path, doc_id).map(|mut p| {
        let mut name = p.file_name().unwrap_or_default().to_os_string();
        name.push(TEMP_SUFFIX);
        p.set_file_name(name);
        p
    })
}

#[cfg(test)]
mod tests {
    use super::DocId;

    #[test]
    fn a_generated_id_is_32_lowercase_hex_characters() {
        let id = DocId::generate();
        assert_eq!(id.as_str().len(), 32);
        assert!(DocId::from_hex(id.as_str()).is_some());
    }

    #[test]
    fn two_generated_ids_differ() {
        assert_ne!(DocId::generate(), DocId::generate());
    }

    #[test]
    fn a_valid_id_round_trips() {
        let text = "3f2ac91b4d5e6f708192a3b4c5d6e7f8";
        assert_eq!(DocId::from_hex(text).expect("valid").as_str(), text);
    }

    #[test]
    fn ids_that_could_steer_a_write_out_of_the_directory_are_rejected() {
        assert!(DocId::from_hex("../../../etc/passwd").is_none());
        assert!(DocId::from_hex("3f2ac91b4d5e6f70/192a3b4c5d6e7f8").is_none());
    }

    #[test]
    fn wrong_length_or_case_or_alphabet_is_rejected() {
        assert!(DocId::from_hex("3f2a").is_none(), "too short");
        assert!(
            DocId::from_hex("3f2ac91b4d5e6f708192a3b4c5d6e7f80").is_none(),
            "too long"
        );
        assert!(
            DocId::from_hex("3F2AC91B4D5E6F708192A3B4C5D6E7F8").is_none(),
            "uppercase — one id must have exactly one spelling, or a case-insensitive \
             filesystem gets two files for one document"
        );
        assert!(
            DocId::from_hex("g2ac91b4d5e6f708192a3b4c5d6e7f8z").is_none(),
            "non-hex"
        );
    }
}
