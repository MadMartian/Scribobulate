//! Pure, display-free decision cores (unit-tested): action-enable predicates, the
//! window-title / tab-label formulas, the save-safety check, and the
//! external-file-change action.

use super::ViewMode;

/// Whether an editor-only Edit command should be enabled: only when the editor is
/// visible (edit or split mode) AND the command has something to act on.
///
/// `has_target` is the command family's own precondition, evaluated by the
/// caller: an editor **selection** for Cut / Delete / Change Case, a Markdown
/// **link under the caret** for Copy Link Location. One predicate rather than one
/// per family, because the mode half — "the editor is not even on screen" — is
/// the same fact for all of them, and a second copy of it is how one command ends
/// up live in read-only preview after a mode rule changes.
pub(crate) fn edit_actions_enabled(mode: ViewMode, has_target: bool) -> bool {
    mode.is_editor_visible() && has_target
}

/// Whether Save should be enabled. Save writes the editor buffer to disk — a
/// *file-side* operation, NOT a buffer mutation — so it is exempt from the
/// preview-mode mutable-action lockout and is enabled in *every* view mode,
/// including preview-only. Two independent conditions make a document savable:
///
/// - `dirty`: the buffer differs from the on-disk baseline, so there is unsaved
///   work to write.
/// - `backing_lost`: the document HAS a backing path but that file was deleted or
///   truncated out from under the buffer ([`BackingLoss`]). The buffer still holds
///   the document's content, and Save restores the file — so Save must be enabled
///   even though the buffer is byte-for-byte "clean" against a baseline the file no
///   longer holds. Without this a clean document over a lost file could only be
///   recovered via Save As, and the "save to restore it" notice pointed at a control
///   that would not act.
///
/// A clean document whose file is present has nothing to write, so Save is
/// disabled regardless of mode. (Save As is a separate, always-enabled action —
/// it can write a copy to a new path even from a clean, present document.)
pub(crate) fn save_enabled(dirty: bool, backing_lost: bool) -> bool {
    dirty || backing_lost
}

/// Whether Rename should be enabled for a document (TDD 24.6).
///
/// **This predicate is per-TAB, which is why it exists as a function rather than
/// being read off the action.** `win.rename` is window-scoped and answers for
/// whichever tab is active *now*; the tab-strip context menu fires for the
/// **right-clicked** tab, which need not be that one. So the action's own
/// `is_enabled()` is the wrong answer for the context-menu button, and both readers
/// go through here instead — one rule, two readers, exactly as Copy Full Path and
/// Reload already do (`window/tabs/contextmenu.rs`).
///
/// Each condition, and why it is a *precondition* rather than something the
/// operation could recover from:
///
/// - `has_path`: an untitled document has no file to rename. The honest routing is
///   Save As, so the command is simply insensitive rather than silently becoming one.
/// - `!dirty`: renaming a document with unsaved edits would leave the buffer's only
///   copy pointing at a file whose name just changed under it. The operator accepted
///   the resulting cliff — the command greys out and the reader saves first — over a
///   "Save and rename" prompt.
/// - `!backing_lost`: the file is already known to be gone; there is nothing to
///   rename. This gates the **command**; it is deliberately *not* sufficient as the
///   operation's precondition, which re-checks against the filesystem (24.8) because
///   the flag is only set if the monitor happened to observe the deletion.
/// # Why "a write is in flight" is NOT a parameter here
///
/// TDD 24.6 requires Rename to be unavailable while a write to that document is in
/// flight, and the obvious reading is a fourth veto. It is deliberately not one, for
/// two reasons that point the same way:
///
/// 1. **It would be unreachable.** A write is only ever in flight for a document that
///    is `dirty` (a save's baseline is updated *after* the write lands, so the buffer
///    still differs from it throughout) or `backing_lost` (the save-to-restore
///    case). Either one already vetoes. A fourth condition that no reachable state can
///    be the sole cause of is dead code wearing a guard's clothes — and, being a
///    second sufficient mechanism, it would make the other two mutation-proof one at a
///    time (GEP-11): neuter the dirty veto and the suite stays green.
/// 2. **The gate is barely readable by design.** `WriteGate::is_busy` has exactly ONE
///    sanctioned production caller, so no other caller can branch on the state and act
///    on it a moment later — the check-then-act race a `WritePass` exists to make
///    unrepresentable. (This used to say `is_busy` was `#[cfg(test)]`, which its
///    promotion to `pub(crate)` made false; the rule survived the change, the sentence
///    describing it did not.)
///
/// So the in-flight requirement is met where it is actually decidable: the operation
/// **claims** the gate, and a refused claim abandons the rename. Sensitivity is a
/// hint; the pass is the guarantee. This is the plan's "preconditions are re-checked
/// at apply time, not trusted from the gate" applied to the one precondition that
/// cannot be honestly read in advance.
pub(crate) fn rename_enabled(has_path: bool, dirty: bool, backing_lost: bool) -> bool {
    has_path && !dirty && !backing_lost
}

/// Whether a window is a *reusable blank*: no backing file and the editor still
/// holds the untouched WELCOME text (so reusing it cannot clobber unsaved work).
pub(crate) fn is_blank_welcome(path_present: bool, buffer_text: &str, welcome: &str) -> bool {
    !path_present && buffer_text == welcome
}

/// The application's display name, as it appears to a user: in a window title,
/// in a taskbar or window switcher, and as the caption of a modal dialog.
///
/// One definition rather than a literal per site — the same no-drift reasoning as
/// the title formula below, which is its main consumer. It is deliberately NOT the
/// application **ID** (`icons.rs` owns that, and the two are different strings that
/// happen to look related).
pub(crate) const APP_NAME: &str = "Scribobulate";

/// Pure decision core of the window-title formula (operator decisions Q7/Q14,
/// TDD 15.7): **the ACTIVE tab's filename**, plus a parenthetical count of the
/// window's OTHER tabs once there is at least one — `notes.md (+2 documents) —
/// Scribobulate`. An untitled/welcome active tab has no filename to lead with, so
/// the app name takes that slot and carries the count itself (`Scribobulate (+2
/// documents)`), exactly as a lone untitled tab reads a bare `Scribobulate`.
///
/// `others` — `tab_count - 1` — is what the parenthetical counts, not `tab_count`:
/// "+2" means "two more documents *besides* the one named", so it is the count the
/// "+" makes true. It is deliberately singular at one (`(+1 document)`) — a title
/// bar is the most-read string in the app and "+1 documents" is the kind of blemish
/// that outlives every release.
///
/// **This is the only function that produces a window title.** Every path that
/// names a window — open, session restore, link navigation, tab open/close/move/
/// switch, and Save As — resolves through here, directly or via `docio::title_for`,
/// which is a one-tab call to it. A second
/// derivation is how Save As came to set a title without the app-name suffix and
/// without the multi-tab count: it looked correct at its own call site and could
/// only be seen by comparing two places (Derived-view CAM row 4, column B).
///
/// A zero-tab window (transient, about to close) has no active tab and no others,
/// so it reads as the bare app name — `saturating_sub` keeps that case arithmetic,
/// not a special case.
pub(crate) fn window_title_for_tabs(tab_count: usize, active_tab_name: Option<&str>) -> String {
    let others = tab_count.saturating_sub(1);
    let extra = match others {
        0 => String::new(),
        1 => " (+1 document)".to_string(),
        n => format!(" (+{n} documents)"),
    };
    match active_tab_name {
        Some(name) => format!("{name}{extra} — {APP_NAME}"),
        None => format!("{APP_NAME}{extra}"),
    }
}

/// The independent per-tab badge bits that drive a tab's notebook label
/// (operator decision Q7, TDD 15.7). A named struct rather than adjacent
/// same-typed `bool` parameters (QA round-1 M7): a transposed pair of bare
/// bools compiles cleanly and silently swaps which marker (`⚠`/`⟳`/`•`) shows,
/// a defect only catchable by cross-referencing the call site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TabBadgeState {
    /// Unsaved changes (trailing "•").
    pub(crate) dirty: bool,
    /// An external (on-disk) change was decided while this tab was in the
    /// background and is awaiting replay on activation (leading "⟳"; TDD
    /// 15.13).
    pub(crate) pending_external: bool,
    /// The loaded document's backing file was deleted or truncated on disk
    /// (leading, coloured "⚠"; TDD 15.22). The buffer still holds the document's
    /// only copy, so — like a dirty tab — it prompts before closing until saved.
    pub(crate) backing_lost: bool,
}

/// Pure decision core of a tab's own notebook label, rendered as **Pango
/// markup** so the "⚠" badge can be coloured (operator decision Q7, TDD
/// 15.7/15.22): its filename, a leading coloured "⚠" while
/// `badge.backing_lost`, a leading "⟳" while `badge.pending_external`, and a
/// trailing "•" while `badge.dirty`.
///
/// `name` MUST already be Pango-markup-escaped by the caller — a filename can
/// contain `&`/`<`/`>` — since this core interpolates it verbatim into markup.
/// `warn_color` is the caller-supplied `<span foreground=…>` value for the "⚠";
/// keeping it a parameter (rather than a constant here) leaves this core
/// display-free and testable — it decides badge ORDERING, not "which yellow".
pub(crate) fn tab_label_markup(name: &str, badge: TabBadgeState, warn_color: &str) -> String {
    let mut label = String::new();
    if badge.backing_lost {
        label.push_str("<span foreground=\"");
        label.push_str(warn_color);
        label.push_str("\">⚠</span> ");
    }
    if badge.pending_external {
        label.push_str("⟳ ");
    }
    label.push_str(name);
    if badge.dirty {
        label.push_str(" •");
    }
    label
}

/// Pure decision core of the footer's caret line/column indicator (TDD 9.21):
/// `None` when `mode` has no editor pane to report a position for (preview),
/// so the caller hides the label entirely rather than showing a stale or
/// meaningless position; `Some("Ln L, Col C")` (both 1-based) in edit/split,
/// regardless of which pane currently has literal focus — the indicator
/// always reflects the EDITOR buffer's own caret, never the preview's.
pub(crate) fn line_col_indicator(mode: ViewMode, line: i32, visual_col: u32) -> Option<String> {
    if !mode.is_editor_visible() {
        return None;
    }
    Some(format!("Ln {line}, Col {visual_col}"))
}

/// Whether it is safe to overwrite the on-disk file on save: content-gated,
/// not mtime-gated (QA round-1 H3/H4/H5/M6). A coarse filesystem
/// clock (FAT/exFAT 2s, many NFS/SMB 1s resolution), a same-tick external
/// write, or an mtime sampled by a syscall separate from the content it
/// describes could all previously make a genuine external change read as
/// "safe" and get silently clobbered. Comparing actual bytes has none of
/// those failure modes: safe when the on-disk content could not be read at
/// all (deleted / permissions — nothing there to clobber; the write itself
/// will surface any real I/O error) or is byte-identical to `baseline` (the
/// content we last loaded/saved/reloaded FROM disk — never the in-progress
/// edit, which is expected to differ). Any other on-disk content means
/// something else wrote to the file since we last synced with it — except a
/// blank file under a document already flagged as having lost its backing
/// (`backing_lost`): that is the truncation the user is saving to repair, not
/// someone else's work to warn about (TDD 15.22).
pub(crate) fn save_is_safe(baseline: &str, disk_content: Option<&str>, backing_lost: bool) -> bool {
    match disk_content {
        Some(disk) => disk == baseline || (backing_lost && is_blank_content(disk)),
        None => true,
    }
}

/// Why a document's buffer has become the only copy of it (TDD 3.4, 3.5, 15.22).
///
/// One state with two reasons rather than two flags, so that everything guarding the
/// buffer — Save, the ⚠ badge, the close prompt, the crash-recovery snapshot — reads
/// one answer and cannot protect one loss while forgetting the other.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum BackingLoss {
    /// The file is gone from disk.
    Deleted,
    /// The file is present but blank ([`is_blank_content`]).
    Truncated,
}

impl BackingLoss {
    /// The status notice that announces this loss.
    pub(crate) fn notice(self) -> &'static str {
        match self {
            Self::Deleted => "File deleted on disk — save to restore it",
            Self::Truncated => "File was truncated — save to restore it",
        }
    }

    /// The floating prompt's wording for this loss.
    ///
    /// Deliberately NOT [`notice`](Self::notice): that line has to carry its own remedy
    /// because nothing next to it can, whereas this one sits beside a Save button, so
    /// repeating "save to restore it" would name the control the reader is already
    /// looking at. Same fact, two surfaces, and the wording follows what each surface
    /// can do rather than being one string shared for the look of it.
    pub(crate) fn prompt(self) -> &'static str {
        match self {
            Self::Deleted => "File deleted on disk. This window has the only known copy.",
            Self::Truncated => "File was truncated. This window has the only known copy.",
        }
    }
}

/// Every character a file may consist of and still count as blank (TDD 3.5): the
/// residue of a writer that truncated and wrote nothing, or wrote only an empty
/// variable's `echo`.
const BLANK_CHARS: [char; 5] = [' ', '\t', '\n', '\r', '\u{FEFF}'];

/// How long a file must stay blank, or stay gone, before the buffer is treated as its
/// only copy (TDD 3.4, 3.5).
///
/// **Both losses need this wait, and for one reason.** A writer replacing a file passes
/// through a state indistinguishable from having destroyed it — truncating leaves the
/// file blank, and the write-temp-then-`rename` every careful writer performs makes the
/// watched path momentarily absent and, on Linux and Windows, reports a `DELETED` event
/// whether or not it was ever absent (`glocalfilemonitor.c` expands a rename into a
/// delete plus a synthetic create when `WATCH_MOVES` is off). A writer that succeeds is
/// finished within milliseconds; one that failed leaves the file blank or gone
/// indefinitely. So a short wait separates them without the reader noticing it, and
/// nothing may conclude a loss from the first observation alone.
pub(crate) const BACKING_SETTLE: std::time::Duration = std::time::Duration::from_millis(500);

/// Whether `text` holds nothing a reader could lose — see [`BLANK_CHARS`].
pub(crate) fn is_blank_content(text: &str) -> bool {
    text.chars().all(|c| BLANK_CHARS.contains(&c))
}

/// What to do when the backing file is found to have changed on disk.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ExternalChange {
    /// No actionable change — content identical, a dirty buffer whose conflict
    /// notice the user already dismissed, or a flagged document still blank or gone.
    Ignore,
    /// Content changed under a buffer that must not be replaced silently — raise the
    /// conflict toast.
    Toast,
    /// Content changed and the buffer is clean — reload silently.
    Reload,
    /// The file just read blank, or read as gone, over a document that was neither.
    /// Re-read after [`BACKING_SETTLE`] before deciding anything, so a rewrite caught
    /// between its truncate and its write — or between its unlink and its rename —
    /// neither reloads a blank page nor raises a warning.
    AwaitSettle,
    /// Still blank, or still gone, after the settle — keep the buffer and flag the
    /// loss. Carries the *reason* rather than splitting into two variants, for the
    /// same reason [`BackingLoss`] does: one arm applies both, so no caller can handle
    /// one loss and forget the other.
    Lost(BackingLoss),
    /// A flagged document's file is back with exactly the content last loaded or
    /// saved — retire the flag; there is nothing to reload.
    Restored,
}

/// What one read of a document's path found. A read that fails *because the file is
/// not there* is an observation, not an error — it is the whole of how a deletion is
/// detected — whereas every other failure (permissions, transient I/O, a path that is
/// no longer an admissible document) answers nothing and never reaches here.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum DiskRead<'a> {
    /// The file was read, and holds this.
    Content(&'a str),
    /// The file is not there.
    Absent,
}

/// Everything [`external_change_action`] decides from: one read of the file, and the
/// document it was read for.
pub(crate) struct DiskObservation<'a> {
    /// What the read of the file just found.
    pub(crate) disk: DiskRead<'a>,
    /// The text the preview was rendered from.
    pub(crate) source: &'a str,
    /// The content last loaded from or saved to disk.
    pub(crate) baseline: &'a str,
    /// Whether the buffer has unsaved edits.
    pub(crate) dirty: bool,
    /// Whether the user dismissed the conflict notice.
    pub(crate) suppressed: bool,
    /// Whether the buffer is already known to be the only copy.
    pub(crate) backing_loss: Option<BackingLoss>,
    /// Whether this read is the re-read taken after [`BACKING_SETTLE`].
    pub(crate) settled: bool,
}

/// Decide how to respond to an external file change. Pure decision core of
/// `window::check_and_reload_tab` (TDD 3.1, 3.4, 3.5, 3.6, 4.6, 5.1, 9.13).
///
/// A flagged document is decided as though it were dirty whatever its buffer says:
/// its buffer is the only copy of what the file held, so new content is a conflict to
/// raise, never a reload to apply (3.6).
///
/// **A loss is never concluded from one observation.** Absent and blank are the two
/// states a file passes *through* while it is being replaced, so each costs a
/// [`BACKING_SETTLE`] re-read before it means anything — see that constant for why
/// the delete-shaped one is not optional on any platform this runs on.
pub(crate) fn external_change_action(seen: &DiskObservation) -> ExternalChange {
    let disk = match seen.disk {
        DiskRead::Absent => {
            return match (seen.backing_loss, seen.settled) {
                // Already recorded as gone: a backend that reports one deletion twice
                // (Win32 does, for a single replace-existing rename) re-decides nothing.
                (Some(BackingLoss::Deleted), _) => ExternalChange::Ignore,
                // Including a document already flagged Truncated — its file has gone
                // from blank to absent, which is a different loss with a different
                // notice, so it is re-decided rather than left reading "truncated".
                (_, false) => ExternalChange::AwaitSettle,
                (_, true) => ExternalChange::Lost(BackingLoss::Deleted),
            };
        }
        DiskRead::Content(disk) => disk,
    };
    if is_blank_content(disk) && !is_blank_content(seen.baseline) {
        return match (seen.backing_loss, seen.settled) {
            (Some(_), _) => ExternalChange::Ignore,
            (None, false) => ExternalChange::AwaitSettle,
            (None, true) => ExternalChange::Lost(BackingLoss::Truncated),
        };
    }
    if seen.backing_loss.is_some() {
        return if disk == seen.baseline {
            ExternalChange::Restored
        } else if seen.suppressed {
            ExternalChange::Ignore
        } else {
            ExternalChange::Toast
        };
    }
    if disk == seen.source {
        ExternalChange::Ignore
    } else if !seen.dirty {
        ExternalChange::Reload
    } else if seen.suppressed {
        ExternalChange::Ignore
    } else {
        ExternalChange::Toast
    }
}

#[cfg(test)]
mod tests {
    use super::is_blank_content;
    use crate::winstate::*;

    #[test]
    fn edit_actions_need_editor_mode_and_selection() {
        assert!(edit_actions_enabled(ViewMode::Edit, true));
        assert!(edit_actions_enabled(ViewMode::Split, true));
        assert!(!edit_actions_enabled(ViewMode::Edit, false)); // no selection
        assert!(!edit_actions_enabled(ViewMode::Preview, true)); // read-only mode
    }

    #[test]
    fn save_enabled_tracks_dirty_not_mode() {
        // Save is a file-side action: enabled iff there are unsaved changes,
        // in EVERY view mode (including preview-only). Mode never
        // enters the predicate.
        assert!(save_enabled(true, false));
        assert!(!save_enabled(false, false));
    }

    #[test]
    fn save_enabled_when_backing_file_missing_even_if_clean() {
        // A clean buffer whose backing file was deleted on disk is still
        // savable — Save re-creates the file — so the deleted-file
        // "save to restore it" notice's action is live. Independent of dirty:
        // a present-file clean buffer is the only Save-disabled state.
        assert!(save_enabled(false, true)); // clean + backing gone → enabled
        assert!(save_enabled(true, true)); // dirty + backing gone → enabled
        assert!(save_enabled(true, false)); // dirty + present → enabled (unchanged)
        assert!(!save_enabled(false, false)); // clean + present → the only disabled case
    }

    /// TDD 24.6. The truth table is written out in full rather than spot-checked:
    /// the predicate is four independent veto conditions, and a test that only
    /// exercises the happy case plus one veto cannot tell an `&&` from an `||`.
    #[test]
    fn rename_needs_a_file_thats_saved_and_present() {
        // The one enabled state: a titled, clean, present document.
        assert!(rename_enabled(true, false, false));

        // Each veto alone is sufficient to disable.
        assert!(!rename_enabled(false, false, false), "untitled");
        assert!(!rename_enabled(true, true, false), "unsaved changes");
        assert!(!rename_enabled(true, false, true), "backing gone");

        // And no combination of vetoes cancels out.
        assert!(!rename_enabled(false, true, true));
        assert!(!rename_enabled(true, true, true));

        // The in-flight-write requirement of TDD 24.6 is deliberately absent from
        // this predicate and met by claiming the write gate instead — see the
        // function's doc comment. A write in flight always implies `dirty` or
        // `backing_lost`, both asserted above, so the contract is covered here
        // and the guarantee is the `WritePass`.
    }

    #[test]
    fn blank_welcome_requires_no_path_and_untouched_text() {
        assert!(is_blank_welcome(false, "WELCOME", "WELCOME"));
        assert!(!is_blank_welcome(true, "WELCOME", "WELCOME")); // has a file
        assert!(!is_blank_welcome(false, "edited", "WELCOME")); // user typed
    }

    #[test]
    fn save_is_safe_gates_on_content_not_mtime() {
        // QA round-1: content-identical → safe, regardless of what any clock says.
        assert!(save_is_safe("hello", Some("hello"), false));
        // Any other on-disk content → unsafe, even a single-byte difference
        // that a coarse-resolution mtime clock could never have detected.
        assert!(!save_is_safe("hello", Some("hello!"), false));
        // Unreadable/deleted on disk → allow (the write itself surfaces any
        // real I/O error; there is nothing there to clobber).
        assert!(save_is_safe("hello", None, false));
    }

    #[test]
    fn saving_over_a_truncated_file_is_safe_and_over_anything_else_is_not() {
        // TDD 15.22: the blank file is what the save is repairing.
        assert!(save_is_safe("hello", Some(""), true));
        assert!(save_is_safe("hello", Some(" \n\t\r\n"), true));
        // Blank but never flagged: somebody else's write, even an empty one.
        assert!(!save_is_safe("hello", Some(""), false));
        // Flagged, but the file came back with other content: a conflict (TDD 3.6).
        assert!(!save_is_safe("hello", Some("other"), true));
    }

    #[test]
    fn blank_content_is_whitespace_and_a_byte_order_mark_only() {
        for blank in ["", "\n", "\r\n", " \t\n", "\u{FEFF}", "\u{FEFF}\n"] {
            assert!(is_blank_content(blank), "{blank:?} is blank");
        }
        // A non-breaking or zero-width space is a character somebody typed.
        for text in ["x", "\n#\n", "\u{00A0}", "\u{200B}"] {
            assert!(!is_blank_content(text), "{text:?} is content");
        }
    }

    #[test]
    fn each_loss_announces_itself_distinctly() {
        assert_eq!(
            BackingLoss::Deleted.notice(),
            "File deleted on disk — save to restore it"
        );
        assert_eq!(
            BackingLoss::Truncated.notice(),
            "File was truncated — save to restore it"
        );
    }

    /// A clean, unflagged document whose preview source is its baseline, read on the
    /// first (unsettled) pass.
    fn seen<'a>(disk: &'a str, baseline: &'a str) -> DiskObservation<'a> {
        DiskObservation {
            disk: DiskRead::Content(disk),
            source: baseline,
            baseline,
            dirty: false,
            suppressed: false,
            backing_loss: None,
            settled: false,
        }
    }

    fn flagged(loss: BackingLoss, disk: &str) -> DiskObservation<'_> {
        DiskObservation {
            backing_loss: Some(loss),
            ..seen(disk, "doc")
        }
    }

    /// The same document, read back as no longer there.
    fn gone(baseline: &str) -> DiskObservation<'_> {
        DiskObservation {
            disk: DiskRead::Absent,
            ..seen(baseline, baseline)
        }
    }

    #[test]
    fn external_change_action_maps_the_ordinary_matrix() {
        use ExternalChange::*;
        // No on-disk change → never act, regardless of dirty/suppressed.
        assert_eq!(external_change_action(&seen("doc", "doc")), Ignore);
        let dirty_same = DiskObservation {
            dirty: true,
            ..seen("doc", "doc")
        };
        assert_eq!(external_change_action(&dirty_same), Ignore);
        // Changed + clean buffer → silent reload (suppressed is irrelevant).
        assert_eq!(external_change_action(&seen("new", "doc")), Reload);
        let suppressed = DiskObservation {
            suppressed: true,
            ..seen("new", "doc")
        };
        assert_eq!(external_change_action(&suppressed), Reload);
        // Changed + dirty buffer → toast, unless the user dismissed the notice.
        let dirty = DiskObservation {
            dirty: true,
            ..seen("new", "doc")
        };
        assert_eq!(external_change_action(&dirty), Toast);
        let dismissed = DiskObservation {
            suppressed: true,
            ..dirty
        };
        assert_eq!(external_change_action(&dismissed), Ignore);
    }

    /// TDD 3.5. The first blank read must neither reload a blank page nor warn: a
    /// rewrite caught between its truncate and its write looks exactly like this.
    #[test]
    fn a_blank_read_is_a_truncation_only_once_it_survives_the_settle() {
        use ExternalChange::*;
        assert_eq!(external_change_action(&seen("", "doc")), AwaitSettle);
        assert_eq!(external_change_action(&seen("\n", "doc")), AwaitSettle);
        let dirty = DiskObservation {
            dirty: true,
            ..seen(" \n", "doc")
        };
        assert_eq!(external_change_action(&dirty), AwaitSettle);

        let still_blank = DiskObservation {
            settled: true,
            ..seen("", "doc")
        };
        assert_eq!(
            external_change_action(&still_blank),
            Lost(BackingLoss::Truncated)
        );
        let dirty_dismissed = DiskObservation {
            settled: true,
            dirty: true,
            suppressed: true,
            ..seen("\n", "doc")
        };
        assert_eq!(
            external_change_action(&dirty_dismissed),
            Lost(BackingLoss::Truncated),
            "unsaved edits and a dismissed prompt do not stop the warning"
        );

        // Refilled by the time the settle re-read ran: an ordinary change.
        let refilled = DiskObservation {
            settled: true,
            ..seen("new", "doc")
        };
        assert_eq!(external_change_action(&refilled), Reload);

        // A blank document rewritten blank is content, never a truncation.
        assert_eq!(external_change_action(&seen("\n", "")), Reload);
        let blank_again = DiskObservation {
            settled: true,
            ..seen("", "")
        };
        assert_eq!(external_change_action(&blank_again), Ignore);
    }

    /// TDD 3.4, and the twin of the blank-read case above. A read that comes back
    /// absent is the *same* ambiguity: every careful external writer replaces a file
    /// by renaming a temp over it, which reports as a deletion (and is briefly one),
    /// so concluding a loss from the first absent read condemns ordinary saves by
    /// vim, VS Code and `git checkout` as deletions.
    #[test]
    fn an_absent_read_is_a_deletion_only_once_it_survives_the_settle() {
        use ExternalChange::*;
        assert_eq!(external_change_action(&gone("doc")), AwaitSettle);
        let dirty = DiskObservation {
            dirty: true,
            ..gone("doc")
        };
        assert_eq!(
            external_change_action(&dirty),
            AwaitSettle,
            "unsaved edits do not make the first absent read conclusive either"
        );

        let still_gone = DiskObservation {
            settled: true,
            ..gone("doc")
        };
        assert_eq!(
            external_change_action(&still_gone),
            Lost(BackingLoss::Deleted)
        );

        // The replacement landed before the settle re-read ran: an ordinary change,
        // and the negative control for the whole mechanism (TDD 3.1).
        let replaced = DiskObservation {
            settled: true,
            ..seen("new", "doc")
        };
        assert_eq!(external_change_action(&replaced), Reload);
    }

    /// The two ways an already-flagged document answers an absent read.
    #[test]
    fn an_absent_read_over_a_flagged_document_re_decides_only_the_other_loss() {
        use ExternalChange::*;
        // Win32 reports ONE replace-existing rename as TWO `Deleted`s; a second
        // observation of a deletion already recorded must decide nothing.
        let already = DiskObservation {
            backing_loss: Some(BackingLoss::Deleted),
            ..gone("doc")
        };
        assert_eq!(external_change_action(&already), Ignore);
        let already_settled = DiskObservation {
            settled: true,
            ..already
        };
        assert_eq!(external_change_action(&already_settled), Ignore);

        // A truncated file that is then removed is a different loss with a different
        // notice, so it is re-decided rather than left reading "truncated".
        let emptied_then_removed = DiskObservation {
            backing_loss: Some(BackingLoss::Truncated),
            ..gone("doc")
        };
        assert_eq!(external_change_action(&emptied_then_removed), AwaitSettle);
        let settled = DiskObservation {
            settled: true,
            ..emptied_then_removed
        };
        assert_eq!(external_change_action(&settled), Lost(BackingLoss::Deleted));
    }

    /// TDD 3.6, for both losses: a flagged document is decided as though dirty.
    #[test]
    fn a_flagged_document_is_restored_only_by_its_own_content() {
        use ExternalChange::*;
        for loss in [BackingLoss::Deleted, BackingLoss::Truncated] {
            assert_eq!(
                external_change_action(&flagged(loss, "doc")),
                Restored,
                "{loss:?}"
            );
            assert_eq!(
                external_change_action(&flagged(loss, "new")),
                Toast,
                "{loss:?}: a clean buffer over a lost file is still never reloaded"
            );
            let dismissed = DiskObservation {
                suppressed: true,
                ..flagged(loss, "new")
            };
            assert_eq!(external_change_action(&dismissed), Ignore, "{loss:?}");
            assert_eq!(
                external_change_action(&flagged(loss, "")),
                Ignore,
                "{loss:?}: a return that is still blank changes nothing"
            );
            let settled = DiskObservation {
                settled: true,
                ..flagged(loss, "\n")
            };
            assert_eq!(external_change_action(&settled), Ignore, "{loss:?}");
        }
    }

    #[test]
    fn window_title_names_the_active_tab_and_counts_the_others() {
        // One tab: the active tab's name and nothing else — no "(+0 documents)".
        assert_eq!(
            window_title_for_tabs(1, Some("foo.md")),
            "foo.md — Scribobulate"
        );
        assert_eq!(window_title_for_tabs(1, None), "Scribobulate");
        // The parenthetical counts the OTHER documents, so it is one less than the
        // tab count — three tabs means two besides the one named.
        assert_eq!(
            window_title_for_tabs(3, Some("foo.md")),
            "foo.md (+2 documents) — Scribobulate"
        );
        assert_eq!(
            window_title_for_tabs(9, Some("foo.md")),
            "foo.md (+8 documents) — Scribobulate"
        );
        // Singular at exactly one other — never "(+1 documents)".
        assert_eq!(
            window_title_for_tabs(2, Some("foo.md")),
            "foo.md (+1 document) — Scribobulate"
        );
        // An untitled active tab still reports its siblings; the app name takes the
        // name slot (as it does for a lone untitled tab) rather than the count
        // silently disappearing with the filename.
        assert_eq!(window_title_for_tabs(2, None), "Scribobulate (+1 document)");
        assert_eq!(
            window_title_for_tabs(4, None),
            "Scribobulate (+3 documents)"
        );
        assert_eq!(window_title_for_tabs(0, None), "Scribobulate"); // transient zero-tab window
    }

    // A fixed sentinel colour so the ordering assertions don't hard-code a
    // particular yellow — the display layer owns "which yellow" (documents.rs).
    const C: &str = "#c";

    fn badge(dirty: bool, pending_external: bool, backing_lost: bool) -> TabBadgeState {
        TabBadgeState {
            dirty,
            pending_external,
            backing_lost,
        }
    }

    #[test]
    fn tab_label_shows_filename_and_dirty_marker() {
        assert_eq!(
            tab_label_markup("foo.md", badge(false, false, false), C),
            "foo.md"
        );
        assert_eq!(
            tab_label_markup("foo.md", badge(true, false, false), C),
            "foo.md •"
        );
        assert_eq!(
            tab_label_markup("Untitled", badge(false, false, false), C),
            "Untitled"
        );
        assert_eq!(
            tab_label_markup("Untitled", badge(true, false, false), C),
            "Untitled •"
        );
    }

    #[test]
    fn tab_label_shows_pending_external_change_badge() {
        // TDD 15.13: a background tab whose own file changed on
        // disk is badged, independent of (and combinable with) the dirty marker.
        assert_eq!(
            tab_label_markup("foo.md", badge(false, true, false), C),
            "⟳ foo.md"
        );
        assert_eq!(
            tab_label_markup("foo.md", badge(true, true, false), C),
            "⟳ foo.md •"
        );
        assert_eq!(
            tab_label_markup("Untitled", badge(false, true, false), C),
            "⟳ Untitled"
        );
    }

    #[test]
    fn tab_label_shows_coloured_deleted_backing_badge() {
        // TDD 15.22: a tab whose backing file was deleted carries a leading
        // coloured "⚠", combinable with the dirty marker, and takes precedence
        // (leftmost) over the "⟳" reload badge — the caller-supplied colour is
        // interpolated verbatim into the span.
        assert_eq!(
            tab_label_markup("foo.md", badge(false, false, true), C),
            "<span foreground=\"#c\">⚠</span> foo.md"
        );
        assert_eq!(
            tab_label_markup("foo.md", badge(true, false, true), C),
            "<span foreground=\"#c\">⚠</span> foo.md •"
        );
        // ⚠ leads ⟳ when (improbably) both are set.
        assert_eq!(
            tab_label_markup("foo.md", badge(false, true, true), C),
            "<span foreground=\"#c\">⚠</span> ⟳ foo.md"
        );
    }

    #[test]
    fn line_col_indicator_hides_in_preview_and_shows_in_edit_and_split() {
        // TDD 9.21: no editor pane in pure preview → hidden regardless of position.
        assert_eq!(line_col_indicator(ViewMode::Preview, 5, 3), None);
        assert_eq!(
            line_col_indicator(ViewMode::Edit, 5, 3),
            Some("Ln 5, Col 3".to_string())
        );
        assert_eq!(
            line_col_indicator(ViewMode::Split, 1, 1),
            Some("Ln 1, Col 1".to_string())
        );
    }
}
