//! Whether a document's live-reload monitor still watches the folder that stands at its
//! path — the state and the pure decision behind `window::rewatch` (TDD 3.4, 3.6).
//!
//! Only consulted where the file monitor is bound to the folder it was attached in rather
//! than to the path (`platform::doc_monitor_is_bound_to_its_folder` — Windows). There, a
//! folder deleted or moved away and recreated leaves the monitor watching a directory
//! that is gone, so the application has to notice the replacement itself and attach a new
//! one. The noticing is a poll of the folder's identity; this module decides what each
//! answer means.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;

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
    /// what concludes a deletion (TDD 3.4).
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
}

impl Default for FolderWatch {
    fn default() -> Self {
        Self {
            dir: RefCell::new(None),
            recorded: Cell::new(WatchedFolder::Unknown),
            generation: Cell::new(0),
            polling: Cell::new(false),
            probing: Cell::new(false),
        }
    }
}

impl FolderWatch {
    /// A monitor was just attached in `dir`: forget what was known about the previous one.
    pub(crate) fn restart(&self, dir: PathBuf) {
        *self.dir.borrow_mut() = Some(dir);
        self.recorded.set(WatchedFolder::Unknown);
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
        let before = watch.generation.get();
        watch.restart(PathBuf::from("docs"));
        assert_eq!(watch.recorded.get(), Unknown);
        assert_eq!(
            watch.dir.borrow().as_deref(),
            Some(std::path::Path::new("docs"))
        );
        assert_ne!(watch.generation.get(), before);
    }
}
