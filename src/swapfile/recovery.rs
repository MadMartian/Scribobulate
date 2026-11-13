//! The recovery decisions — pure functions over plain data, so the whole policy is
//! testable without a display, a filesystem or a crash.

use super::{DocId, SwapHeader};

/// What the invariant demands of a document's swap file right now.
///
/// The enum exists so the invariant has exactly one spelling in the tree. A `bool` would
/// have worked and would have been re-derived, slightly differently, at each of save /
/// save-as / discard / reload / revert — which is the failure this design exists to
/// avoid (GTK4Rs/AP-108, GEP-25).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SwapSync {
    /// The document is dirty: a snapshot must exist.
    Write,
    /// The document is clean: no snapshot may exist.
    Delete,
}

/// **The governing invariant, in one place**: a swap file exists for a document if and
/// only if its buffer is at risk — it is dirty, or its file was deleted or truncated so
/// the buffer is the only copy (TDD 22.18). The caller answers that as "would closing
/// it prompt to save?".
///
/// Every deletion rule in the feature is this function's `Delete` arm. Saving,
/// undoing back to the on-disk content, reverting and reloading all reach it the same
/// way — by taking the document out of risk — so none of them needs its own rule, and a
/// *future* path that changes it inherits the behaviour without being taught it.
pub(crate) fn sync_action(at_risk: bool) -> SwapSync {
    if at_risk {
        SwapSync::Write
    } else {
        SwapSync::Delete
    }
}

/// What the recovery pass should do with one swap file it found.
#[derive(Clone, PartialEq, Debug)]
pub(crate) enum SwapDisposition {
    /// Another live instance owns this snapshot. Skip it and do not touch the file.
    OwnedByLiveInstance,
    /// A restored tab carries this document id: apply the content into that tab.
    ApplyToRestored(DocId),
    /// Nothing restored it, but it names a file: open that file and apply the content.
    ///
    /// Reachable when the crash landed between the snapshot write and the session write,
    /// or when the window was closed in a way that never persisted. Under the
    /// *self-sufficiency* principle this is a first-class case, not an anomaly — the set
    /// of documents to recover is decided from the headers, so a header the session file
    /// has never heard of is still a document to recover.
    ReopenFile(String),
    /// Nothing restored it and it names no file: a recovered untitled document.
    ///
    /// Also where a document whose path was not representable as text lands. That is why
    /// this case must never be inferred from a missing path alone — see
    /// [`SwapHeader::untitled`].
    ReopenUntitled,
}

/// Decide what to do with one swap file.
///
/// **Header-first, session-as-a-hint.** `restored` says only *where to put* recovered
/// content; it never decides *whether* there is content to recover. Reversing that
/// would make a session file that lost a tab silently discard that tab's unsaved work,
/// which is the exact failure this feature exists to prevent.
///
/// # Why identity alone is not enough to find the tab
///
/// A [`DocId`] is 128 random bits minted per tab, so it correlates a snapshot with a tab
/// the **session** restored and with nothing else. A document opened any other way gets a
/// fresh one — and the ordinary way a user reopens a document after a crash is to open the
/// file again (an Explorer double-click, `scribobulate notes.md`, a desktop association),
/// which is exactly that path. The snapshot then matched no restored id, fell through to
/// [`SwapDisposition::ReopenFile`], and the user got **two tabs of one file**: the one they
/// asked for, and the recovered one beside it.
///
/// So `tab_at_same_path` is the second way in: the id of an already-open tab backing the
/// same file, if there is one. The caller resolves it (it is a filesystem question —
/// canonicalising away `..`, symlinks and Windows' case-insensitivity — and this module
/// touches no filesystem), and it is deliberately a *fallback* rather than a first test,
/// because identity is exact where a path is merely suggestive.
///
/// Two constraints on the caller, both load-bearing:
///
/// * **Never offer a tab that an earlier snapshot in the same pass already claimed.** Two
///   snapshots naming one path with different ids are two genuinely distinct unsaved
///   buffers (reachable through two `--new-instance` processes). Letting the second adopt
///   the tab the first just recovered into would silently overwrite recovered work with
///   other recovered work, which is worse than the duplicate tab this rule exists to
///   remove. With the tab withheld, the second correctly falls through and opens its own.
///   "Recovered into" includes a tab the first snapshot *opened* this pass, not only one
///   it was applied to: with no session, that new tab is the file's only one.
/// * **Untitled snapshots are excluded here by construction** — they name no file, so
///   there is nothing to match, and inferring one from a coincidence of emptiness would
///   merge unrelated buffers.
pub(crate) fn disposition(
    header: &SwapHeader,
    owner_is_live: bool,
    restored: &[DocId],
    tab_at_same_path: Option<&DocId>,
) -> SwapDisposition {
    if owner_is_live {
        return SwapDisposition::OwnedByLiveInstance;
    }
    if restored.contains(&header.doc_id) {
        return SwapDisposition::ApplyToRestored(header.doc_id.clone());
    }
    match &header.path {
        Some(path) if !header.untitled => match tab_at_same_path {
            Some(id) => SwapDisposition::ApplyToRestored(id.clone()),
            None => SwapDisposition::ReopenFile(path.clone()),
        },
        _ => SwapDisposition::ReopenUntitled,
    }
}

/// Put every snapshot the session restored by identity ahead of every one that can only
/// be placed by path, keeping the scan's order within each group.
///
/// `disposition`'s path fallback ADOPTS the tab it picks, re-filing it under the
/// snapshot's id. Visited first, a path match can therefore take the very tab another
/// snapshot later in the pass owns by identity — and that owner, still listed in
/// `restored`, then looks for a tab answering to its id, finds none, and is skipped
/// without a word: one tab showing one edit, the other edit unreachable (TDD 22.16).
/// Directory order decides which comes first, so the defect came and went by filesystem.
/// Settling every exact match before any adoption leaves the path fallback only tabs no
/// snapshot owns, and the claimed-tab rule then sends a second same-path snapshot to a
/// tab of its own.
pub(crate) fn identity_matches_first<T>(
    snapshots: &mut [T],
    restored: &[DocId],
    id_of: impl Fn(&T) -> &DocId,
) {
    snapshots.sort_by_key(|s| !restored.contains(id_of(s)));
}

/// Whether the twin on disk still matches the baseline this snapshot was taken against.
///
/// `on_disk` is `None` when the file no longer exists, which counts as *changed*: the
/// recovered content cannot be reconciled against a file that is gone, so it must not
/// be applied as though nothing had happened.
pub(crate) fn baseline_is_current(header: &SwapHeader, on_disk: Option<&[u8]>) -> bool {
    match on_disk {
        Some(bytes) => super::content_digest(bytes) == header.baseline_digest,
        None => false,
    }
}

/// Whether a snapshot holds nothing the file on disk does not already hold (TDD 22.19).
///
/// Such a snapshot is removed rather than recovered. Applying it would leave the tab
/// clean, and a clean tab is never saved, discarded or edited back to its baseline — the
/// only routes that delete a snapshot — so it would be re-applied on every launch, and
/// once the file changed elsewhere it would put the old text back over the new. One
/// arises when a document whose file vanished was snapshotted (TDD 22.18) and the file
/// came back with the same content before the next launch.
///
/// Exact bytes, deliberately: a snapshot that differs in any byte is recovered as before,
/// and losing the user's work is the failure this feature exists to prevent. An untitled
/// snapshot has no file to compare with and is never redundant.
pub(crate) fn holds_nothing_new(header: &SwapHeader, body: &str, on_disk: Option<&[u8]>) -> bool {
    !header.untitled && header.path.is_some() && on_disk == Some(body.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::{
        baseline_is_current, disposition, holds_nothing_new, identity_matches_first, sync_action,
        SwapDisposition, SwapSync,
    };
    use crate::swapfile::{content_digest, DocId, SwapHeader};

    fn id(nibble: char) -> DocId {
        DocId::from_hex(&nibble.to_string().repeat(32)).expect("valid")
    }

    fn header(doc: DocId) -> SwapHeader {
        SwapHeader {
            doc_id: doc,
            path: Some("/home/u/notes.md".to_string()),
            untitled: false,
            baseline_digest: content_digest(b"on disk"),
            written_at: 0,
            owner_pid: 1,
            app_version: "0.1.0".to_string(),
        }
    }

    #[test]
    fn a_dirty_document_requires_a_swap_and_a_clean_one_forbids_it() {
        assert_eq!(sync_action(true), SwapSync::Write);
        assert_eq!(sync_action(false), SwapSync::Delete);
    }

    #[test]
    fn a_restored_tab_gets_its_content_applied_in_place() {
        let h = header(id('a'));
        assert_eq!(
            disposition(&h, false, &[id('a'), id('b')], None),
            SwapDisposition::ApplyToRestored(id('a'))
        );
    }

    #[test]
    fn a_document_the_session_never_restored_is_still_recovered() {
        // The rubric that makes the header authoritative rather than advisory.
        let h = header(id('a'));
        assert_eq!(
            disposition(&h, false, &[id('b')], None),
            SwapDisposition::ReopenFile("/home/u/notes.md".to_string())
        );
    }

    /// A snapshot whose identity nothing restored, but whose FILE is already open, is
    /// applied into that tab rather than opening a second one for the same path.
    ///
    /// This is the ordinary post-crash reopen: the user opens the document again (an
    /// Explorer double-click, a command-line argument, a desktop association), which mints
    /// a fresh id, so identity alone can never correlate the two.
    #[test]
    fn a_snapshot_whose_file_is_already_open_adopts_that_tab() {
        let h = header(id('a'));
        assert_eq!(
            disposition(&h, false, &[id('b')], Some(&id('b'))),
            SwapDisposition::ApplyToRestored(id('b')),
            "the tab already showing this file is the one the work belongs in"
        );
    }

    /// Identity beats the path when both are available — a path is suggestive, an id is
    /// exact, and preferring the path could steal a tab from the snapshot that really
    /// owns it.
    #[test]
    fn an_exact_identity_match_wins_over_a_path_match() {
        let h = header(id('a'));
        assert_eq!(
            disposition(&h, false, &[id('a')], Some(&id('c'))),
            SwapDisposition::ApplyToRestored(id('a'))
        );
    }

    /// The caller withholds a tab an earlier snapshot already claimed, and this is what
    /// the withholding must produce: the second snapshot opens its own tab rather than
    /// overwriting the first one's recovered content.
    #[test]
    fn a_second_snapshot_for_one_path_reopens_rather_than_stealing_the_claimed_tab() {
        let h = header(id('a'));
        assert_eq!(
            disposition(&h, false, &[id('b')], None),
            SwapDisposition::ReopenFile("/home/u/notes.md".to_string()),
            "two unsaved buffers for one path are two documents; losing one to the \
             other is worse than the duplicate tab the path fallback exists to remove"
        );
    }

    /// A path-only snapshot scanned ahead of the one the session restored by identity
    /// must not get first pick: it would adopt the owner's tab and the owner would find
    /// none (TDD 22.16).
    #[test]
    fn snapshots_restored_by_identity_are_placed_before_any_path_match() {
        let mut scanned = vec![id('b'), id('c'), id('a'), id('d')];
        identity_matches_first(&mut scanned, &[id('a'), id('d')], |d| d);
        assert_eq!(
            scanned,
            vec![id('a'), id('d'), id('b'), id('c')],
            "identity matches first, scan order kept within each group"
        );
    }

    #[test]
    fn an_untitled_document_the_session_never_restored_comes_back_as_untitled() {
        let mut h = header(id('a'));
        h.path = None;
        h.untitled = true;
        assert_eq!(
            disposition(&h, false, &[], None),
            SwapDisposition::ReopenUntitled
        );
    }

    /// An untitled snapshot names no file, so it can never adopt a tab by path — even
    /// when the caller offers one. Merging an untitled buffer into somebody's open
    /// document on a coincidence would be a data-loss bug wearing a convenience's clothes.
    #[test]
    fn an_untitled_snapshot_never_adopts_a_tab_by_path() {
        let mut h = header(id('a'));
        h.path = None;
        h.untitled = true;
        assert_eq!(
            disposition(&h, false, &[id('b')], Some(&id('b'))),
            SwapDisposition::ReopenUntitled
        );
    }

    #[test]
    fn a_titled_document_whose_path_is_unrepresentable_does_not_become_untitled_silently() {
        // `untitled` stays false and the path is absent — the document is recovered into
        // a pathless tab so a later save cannot write it somewhere wrong, but nothing
        // claims the user never saved it.
        let mut h = header(id('a'));
        h.path = None;
        h.untitled = false;
        assert_eq!(
            disposition(&h, false, &[], None),
            SwapDisposition::ReopenUntitled
        );
        assert!(!h.untitled, "the header still records that it had a file");
    }

    #[test]
    fn a_live_owner_wins_over_every_other_branch() {
        let h = header(id('a'));
        assert_eq!(
            disposition(&h, true, &[id('a')], Some(&id('b'))),
            SwapDisposition::OwnedByLiveInstance,
            "even a matching restored tab must not steal another instance's snapshot"
        );
    }

    #[test]
    fn an_unchanged_twin_is_current() {
        let h = header(id('a'));
        assert!(baseline_is_current(&h, Some(b"on disk")));
    }

    #[test]
    fn a_twin_changed_since_the_crash_is_not_current() {
        let h = header(id('a'));
        assert!(!baseline_is_current(&h, Some(b"edited by something else")));
    }

    #[test]
    fn a_twin_that_no_longer_exists_counts_as_changed() {
        let h = header(id('a'));
        assert!(
            !baseline_is_current(&h, None),
            "a deleted file must route into the conflict flow, not apply silently"
        );
    }

    #[test]
    fn a_snapshot_identical_to_its_file_holds_nothing_new() {
        let h = header(id('a'));
        assert!(holds_nothing_new(&h, "on disk", Some(b"on disk")));
    }

    #[test]
    fn a_snapshot_differing_in_any_byte_is_still_recovered() {
        let h = header(id('a'));
        assert!(!holds_nothing_new(&h, "on disk\n", Some(b"on disk")));
        assert!(!holds_nothing_new(&h, "on disk", Some(b"on disk\n")));
    }

    #[test]
    fn a_snapshot_whose_file_is_gone_is_the_only_copy() {
        let h = header(id('a'));
        assert!(
            !holds_nothing_new(&h, "", None),
            "an empty snapshot of a deleted file is still recovered (TDD 22.18)"
        );
    }

    #[test]
    fn an_untitled_snapshot_is_never_redundant() {
        let mut h = header(id('a'));
        h.path = None;
        h.untitled = true;
        assert!(!holds_nothing_new(&h, "on disk", Some(b"on disk")));
    }
}
