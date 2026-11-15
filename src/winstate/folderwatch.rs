//! Whether a document's live-reload monitor still watches the folder that stands at its
//! path — the state and the pure decision behind `window::rewatch` (TDD 3.4, 3.6).
//!
//! Only consulted where the file monitor is bound to the folder it was attached in rather
//! than to the path (`platform::doc_monitor_is_bound_to_its_folder` — Windows). There, a
//! folder deleted or moved away and recreated leaves the monitor watching a directory
//! that is gone, so the application has to notice the replacement itself and attach a new
//! one. The noticing is a poll of the folder's identity; this module decides what each
//! answer means.
//!
//! Where the monitor never fires at all (`platform::doc_monitor_is_blind_at`: a document
//! opened through a Windows share), the same poll also stamps the file itself, and a
//! stamp that moves is the change the monitor would have reported ([`file_watch_action`]).
//! Where the monitor does fire, the poll stamps the file only while the document is recorded
//! deleted, because the monitor misses the file's return by rename ([`lost_file_baseline`]).

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::time::SystemTime;

use crate::platform::FolderIdentity;

/// What the last probe found of the folder the current monitor was attached in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WatchedFolder<I> {
    /// Not probed since the monitor attached.
    Unknown,
    /// This folder stands at the path.
    Present(I),
    /// Nothing stands at the path.
    Gone,
}

/// What to do about one probe's answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FolderWatchAction {
    /// Nothing changed that the monitor cannot see for itself.
    Nothing,
    /// The folder is gone. Re-read the file, because a folder moved away (Explorer's
    /// delete into the Recycle Bin) gives the monitor no event at all, and the re-read is
    /// what concludes a deletion (TDD 3.4). Also what a moved [`FileStamp`] asks for,
    /// where the file has no monitor that fires ([`file_watch_action`]).
    Recheck,
    /// A folder the monitor never saw stands at the path. Attach a new monitor there, then
    /// re-read, since whatever was written before the new monitor was watching is not an
    /// event it will ever report (TDD 3.1, 3.6).
    Rearm,
}

/// Decide what `seen` — the folder at the path now, `None` when nothing is there — means
/// against what was `recorded`, and what to record next.
///
/// A folder that comes back after being seen gone is re-armed whatever its identity says:
/// on a filesystem that reuses identities (FAT), a recreated folder can answer the same as
/// the one it replaced, and the monitor is still watching the deleted one.
pub(crate) fn folder_watch_action<I: PartialEq + Copy>(
    recorded: WatchedFolder<I>,
    seen: Option<I>,
) -> (WatchedFolder<I>, FolderWatchAction) {
    use FolderWatchAction::{Nothing, Rearm, Recheck};
    use WatchedFolder::{Gone, Present, Unknown};
    match (recorded, seen) {
        // The monitor attached to whatever was there; that is the baseline.
        (Unknown, Some(now)) => (Present(now), Nothing),
        // Attached while the folder was already missing: the monitor watches nothing, and
        // the read that attach was part of has already answered for the file.
        (Unknown, None) => (Gone, Nothing),
        (Present(was), Some(now)) if was == now => (recorded, Nothing),
        (Present(_) | Gone, Some(now)) => (Present(now), Rearm),
        (Present(_), None) => (Gone, Recheck),
        (Gone, None) => (Gone, Nothing),
    }
}

/// What a poll can see of a file without reading it: whether a write, a truncation or a
/// replacement happened since the last look. The size and the modification time catch a
/// write in place; the creation time catches a file replaced by another that kept the old
/// one's size and modification time (a copy that preserves timestamps). A change this
/// misses entirely leaves all three alone, which no writer of new content does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FileStamp {
    len: u64,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
}

impl FileStamp {
    pub(crate) fn of(meta: &std::fs::Metadata) -> Self {
        Self {
            len: meta.len(),
            modified: meta.modified().ok(),
            created: meta.created().ok(),
        }
    }
}

/// What the last poll found of the document file itself, where no monitor reports on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WatchedFile<S> {
    /// Not polled since the monitor attached.
    Unknown,
    /// The file stood at the path with this stamp.
    Present(S),
    /// Nothing stood at the path.
    Gone,
}

/// Decide whether the file's stamp `seen` — `None` when nothing is at the path — is a change
/// since `recorded`, and what to record next. A change is re-read exactly as a monitor event
/// would be; the re-read decides whether it is a reload, a conflict or a loss (TDD 3.1–3.6).
///
/// The first answer after an attach is the baseline: the attach was part of a read that
/// already answered for the file as it stood.
pub(crate) fn file_watch_action<S: PartialEq + Copy>(
    recorded: WatchedFile<S>,
    seen: Option<S>,
) -> (WatchedFile<S>, FolderWatchAction) {
    use FolderWatchAction::{Nothing, Recheck};
    use WatchedFile::{Gone, Present, Unknown};
    let next = seen.map_or(Gone, Present);
    match (recorded, seen) {
        (Unknown, _) => (next, Nothing),
        (Present(was), Some(now)) if was == now => (recorded, Nothing),
        (Gone, None) => (Gone, Nothing),
        (Present(_) | Gone, _) => (next, Recheck),
    }
}

/// What the poll compares the file's stamp against where the monitor DOES fire, given
/// whether the document is recorded `deleted` (TDD 3.4, 3.6).
///
/// That monitor misses exactly one thing a deleted document needs: on Win32 a file renamed
/// ONTO the watched name reports nothing at all, unless the old name begins with the watched
/// one (the measurement is `window::rewatch`'s). So the poll stamps a deleted
/// document until its file is back, and stops when the loss ends.
///
/// The baseline is [`WatchedFile::Gone`], not [`WatchedFile::Unknown`]: a recorded
/// deletion IS an observation of the file gone, since the settle re-read found nothing. A
/// file back before the first stamp is therefore a change and re-read, where an unknown
/// baseline would take it as the starting point and leave the loss up for good. Without
/// a loss the record is forgotten, so the next loss starts from its own observation.
pub(crate) fn lost_file_baseline<S>(deleted: bool, recorded: WatchedFile<S>) -> WatchedFile<S> {
    match (deleted, recorded) {
        (false, _) => WatchedFile::Unknown,
        (true, WatchedFile::Unknown) => WatchedFile::Gone,
        (true, kept) => kept,
    }
}

/// One tab's folder watch: which folder its monitor was attached in, and what the poll
/// last found there. Re-pointed by `window::rewatch::follow`, which
/// `app::attach_file_backing` — the one place a monitor attaches — calls every time.
#[derive(Debug)]
pub(crate) struct FolderWatch {
    /// The folder the current monitor was attached in; `None` until a monitor attaches.
    pub(crate) dir: RefCell<Option<PathBuf>>,
    /// What the last probe of [`dir`](Self::dir) found.
    pub(crate) recorded: Cell<WatchedFolder<FolderIdentity>>,
    /// Bumped whenever a monitor attaches, so a probe sent out about the previous monitor's
    /// folder is discarded rather than recorded as the new one's baseline.
    pub(crate) generation: Cell<u64>,
    /// Whether this tab's poll timer is running. One per tab, started on the first attach
    /// and stopped by the timer itself once the tab is gone.
    pub(crate) polling: Cell<bool>,
    /// Whether a probe is out. One at a time, so a folder on a hung share costs one held
    /// slot, not one more every tick.
    pub(crate) probing: Cell<bool>,
    /// The document file to stamp on every poll, set only when the monitor just attached
    /// to it never fires (`platform::doc_monitor_is_blind_at`); `None` where the monitor
    /// reports on the file itself.
    pub(crate) file: RefCell<Option<PathBuf>>,
    /// What the last stamp of [`file`](Self::file) found.
    pub(crate) file_recorded: Cell<WatchedFile<FileStamp>>,
}

impl Default for FolderWatch {
    fn default() -> Self {
        Self {
            dir: RefCell::new(None),
            recorded: Cell::new(WatchedFolder::Unknown),
            generation: Cell::new(0),
            polling: Cell::new(false),
            probing: Cell::new(false),
            file: RefCell::new(None),
            file_recorded: Cell::new(WatchedFile::Unknown),
        }
    }
}

impl FolderWatch {
    /// A monitor was just attached in `dir`: forget what was known about the previous one.
    /// `blind_file` is the document's path when that monitor will never fire, so the poll
    /// stamps the file in its place.
    pub(crate) fn restart(&self, dir: PathBuf, blind_file: Option<PathBuf>) {
        *self.dir.borrow_mut() = Some(dir);
        self.recorded.set(WatchedFolder::Unknown);
        *self.file.borrow_mut() = blind_file;
        self.file_recorded.set(WatchedFile::Unknown);
        self.generation.set(self.generation.get().wrapping_add(1));
    }
}

#[cfg(test)]
mod tests {
    use super::FolderWatchAction::{Nothing, Rearm, Recheck};
    use super::WatchedFolder::{Gone, Present, Unknown};
    use super::*;

    #[test]
    fn the_first_answer_after_an_attach_is_the_baseline_and_acts_on_nothing() {
        assert_eq!(folder_watch_action(Unknown, Some(7)), (Present(7), Nothing));
        assert_eq!(folder_watch_action::<u8>(Unknown, None), (Gone, Nothing));
    }

    #[test]
    fn the_same_folder_still_standing_changes_nothing() {
        assert_eq!(
            folder_watch_action(Present(7), Some(7)),
            (Present(7), Nothing)
        );
        assert_eq!(folder_watch_action::<u8>(Gone, None), (Gone, Nothing));
    }

    /// The two measured delete shapes: deleted then recreated, seen gone in between; and replaced
    /// inside one poll interval, seen only as a different folder.
    #[test]
    fn a_folder_the_monitor_never_saw_is_rearmed() {
        assert_eq!(folder_watch_action(Gone, Some(8)), (Present(8), Rearm));
        assert_eq!(
            folder_watch_action(Present(7), Some(8)),
            (Present(8), Rearm)
        );
    }

    #[test]
    fn a_folder_back_after_being_gone_is_rearmed_even_answering_as_before() {
        assert_eq!(
            folder_watch_action(Gone, Some(7)),
            (Present(7), Rearm),
            "an identity-reusing filesystem must not hide the replacement"
        );
    }

    #[test]
    fn a_folder_going_away_is_rechecked_once() {
        assert_eq!(folder_watch_action(Present(7), None), (Gone, Recheck));
        assert_eq!(
            folder_watch_action::<u8>(Gone, None),
            (Gone, Nothing),
            "staying gone re-reads nothing more"
        );
    }

    #[test]
    fn restarting_forgets_the_previous_folder_and_moves_the_generation() {
        let watch = FolderWatch::default();
        watch.recorded.set(Gone);
        watch.file_recorded.set(WatchedFile::Gone);
        let before = watch.generation.get();
        watch.restart(PathBuf::from("docs"), Some(PathBuf::from("plan.md")));
        assert_eq!(watch.recorded.get(), Unknown);
        assert_eq!(watch.file_recorded.get(), WatchedFile::Unknown);
        assert_eq!(
            watch.dir.borrow().as_deref(),
            Some(std::path::Path::new("docs"))
        );
        assert_eq!(
            watch.file.borrow().as_deref(),
            Some(std::path::Path::new("plan.md"))
        );
        assert_ne!(watch.generation.get(), before);

        watch.restart(PathBuf::from("docs"), None);
        assert_eq!(watch.file.borrow().as_deref(), None, "a monitor that fires");
    }

    #[test]
    fn the_first_stamp_after_an_attach_is_the_baseline() {
        use WatchedFile as F;
        assert_eq!(
            file_watch_action(F::Unknown, Some(7)),
            (F::Present(7), Nothing)
        );
        assert_eq!(
            file_watch_action::<u8>(F::Unknown, None),
            (F::Gone, Nothing)
        );
    }

    #[test]
    fn an_unmoved_stamp_rereads_nothing() {
        use WatchedFile as F;
        assert_eq!(
            file_watch_action(F::Present(7), Some(7)),
            (F::Present(7), Nothing)
        );
        assert_eq!(file_watch_action::<u8>(F::Gone, None), (F::Gone, Nothing));
    }

    /// Every way the file can move is one re-read: rewritten, gone, and back.
    #[test]
    fn a_moved_stamp_is_reread_once() {
        use WatchedFile as F;
        assert_eq!(
            file_watch_action(F::Present(7), Some(8)),
            (F::Present(8), Recheck)
        );
        assert_eq!(file_watch_action(F::Present(7), None), (F::Gone, Recheck));
        assert_eq!(
            file_watch_action(F::Gone, Some(8)),
            (F::Present(8), Recheck)
        );
    }

    /// A deleted document's file renamed back before the poll's first stamp is a
    /// change, and so re-read. With an unknown baseline it would be the baseline instead,
    /// and the false "deleted" state would stand.
    #[test]
    fn a_deleted_documents_return_is_a_change_from_its_first_stamp() {
        use WatchedFile as F;
        let baseline = lost_file_baseline::<u8>(true, F::Unknown);
        assert_eq!(
            baseline,
            F::Gone,
            "the recorded deletion is the observation"
        );
        assert_eq!(
            file_watch_action(baseline, Some(7)),
            (F::Present(7), Recheck),
            "back already at the first stamp"
        );
        assert_eq!(file_watch_action(baseline, None), (F::Gone, Nothing));
    }

    /// While the loss stands, what the poll last saw is kept, so a return is re-read once
    /// and not on every tick; once the loss ends it is forgotten, so a later loss starts
    /// from its own observation rather than from a stamp of the file before it.
    #[test]
    fn a_lost_files_record_is_kept_while_lost_and_forgotten_after() {
        use WatchedFile as F;
        assert_eq!(lost_file_baseline(true, F::Present(7)), F::Present(7));
        assert_eq!(
            file_watch_action(lost_file_baseline(true, F::Present(7)), Some(7)),
            (F::Present(7), Nothing),
            "back with a conflict pending: not re-read every tick"
        );
        assert_eq!(lost_file_baseline::<u8>(true, F::Gone), F::Gone);
        assert_eq!(lost_file_baseline(false, F::Present(7)), F::Unknown);
        assert_eq!(lost_file_baseline::<u8>(false, F::Gone), F::Unknown);
    }

    /// The real stamp moves for a write that changes nothing but the content's length, and
    /// stays put when nothing is written.
    #[test]
    fn a_rewrite_moves_the_stamp_of_a_real_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plan.md");
        std::fs::write(&path, "# one\n").unwrap();
        let stamp = || FileStamp::of(&std::fs::metadata(&path).unwrap());
        let first = stamp();
        assert_eq!(stamp(), first, "nothing written, nothing moved");
        std::fs::write(&path, "# one, longer\n").unwrap();
        assert_ne!(stamp(), first);
    }
}
