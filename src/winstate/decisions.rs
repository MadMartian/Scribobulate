//! Pure, display-free decision cores (unit-tested): action-enable predicates, the
//! window-title / tab-label formulas, the save-safety check, and the
//! external-file-change action.

use super::ViewMode;

/// Whether the editor-only Cut/Delete commands should be enabled: only when the
/// editor is visible (edit or split mode) AND there is a selection to act on.
pub(crate) fn edit_actions_enabled(mode: ViewMode, has_selection: bool) -> bool {
    mode.is_editor_visible() && has_selection
}

/// Whether Save should be enabled. Save writes the editor buffer to disk — a
/// *file-side* operation, NOT a buffer mutation — so it is exempt from the
/// preview-mode mutable-action lockout and is enabled in *every* view mode,
/// including preview-only. Two independent conditions make a document savable:
///
/// - `dirty`: the buffer differs from the on-disk baseline, so there is unsaved
///   work to write.
/// - `backing_missing`: the document HAS a backing path but that file is gone
///   from disk (deleted out from under a clean buffer). The buffer still holds
///   the document's content, and Save re-creates the file — so Save must be
///   enabled even though the buffer is byte-for-byte "clean" against a baseline
///   whose file no longer exists. Without this a clean document over a deleted
///   file could only be recovered via Save As, and the "save to restore it"
///   deleted-file notice pointed at a control that would not act.
///
/// A clean document whose file is present has nothing to write, so Save is
/// disabled regardless of mode. (Save As is a separate, always-enabled action —
/// it can write a copy to a new path even from a clean, present document.)
pub(crate) fn save_enabled(dirty: bool, backing_missing: bool) -> bool {
    dirty || backing_missing
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
/// TDD 15.7): a bare count with 2+ tabs (no single filename would be
/// representative), else the sole tab's filename, else the bare app name for
/// an untitled/welcome tab or a transient zero-tab window (about to close).
///
/// **This is the only function that produces a window title.** Every path that
/// names a window — open, session restore, link navigation, tab open/close/move,
/// and Save As — resolves through here, directly or via `docio::title_for`,
/// which is a one-tab call to it. A second
/// derivation is how Save As came to set a title without the app-name suffix and
/// without the multi-tab count: it looked correct at its own call site and could
/// only be seen by comparing two places (Derived-view CAM row 4, column B).
pub(crate) fn window_title_for_tabs(tab_count: usize, single_tab_name: Option<&str>) -> String {
    if tab_count > 1 {
        format!("{tab_count} documents — {APP_NAME}")
    } else {
        match single_tab_name {
            Some(name) => format!("{name} — {APP_NAME}"),
            None => APP_NAME.to_string(),
        }
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
    /// The loaded document's backing file was deleted from disk (leading,
    /// coloured "⚠"; TDD 15.22). The buffer still holds the document's only
    /// copy, so — like a dirty tab — it prompts before closing until saved.
    pub(crate) backing_missing: bool,
}

/// Pure decision core of a tab's own notebook label, rendered as **Pango
/// markup** so the "⚠" badge can be coloured (operator decision Q7, TDD
/// 15.7/15.22): its filename, a leading coloured "⚠" while
/// `badge.backing_missing`, a leading "⟳" while `badge.pending_external`, and a
/// trailing "•" while `badge.dirty`.
///
/// `name` MUST already be Pango-markup-escaped by the caller — a filename can
/// contain `&`/`<`/`>` — since this core interpolates it verbatim into markup.
/// `warn_color` is the caller-supplied `<span foreground=…>` value for the "⚠";
/// keeping it a parameter (rather than a constant here) leaves this core
/// display-free and testable — it decides badge ORDERING, not "which yellow".
pub(crate) fn tab_label_markup(name: &str, badge: TabBadgeState, warn_color: &str) -> String {
    let mut label = String::new();
    if badge.backing_missing {
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
/// something else wrote to the file since we last synced with it.
pub(crate) fn save_is_safe(baseline: &str, disk_content: Option<&str>) -> bool {
    match disk_content {
        Some(disk) => disk == baseline,
        None => true,
    }
}

/// What to do when the backing file is found to have changed on disk.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ExternalChange {
    /// No actionable change — content identical, or a dirty buffer whose conflict
    /// notice the user already dismissed.
    Ignore,
    /// Content changed under a dirty buffer — raise the conflict toast.
    Toast,
    /// Content changed and the buffer is clean — reload silently.
    Reload,
}

/// Decide how to respond to an external file change. Pure decision core of
/// `window::check_and_reload` (TDD 3.1 / 4.6 / 5.1 / 9.13): given whether the
/// on-disk content differs from the in-memory source, whether the buffer has
/// unsaved edits, and whether the user has dismissed the conflict notice.
pub(crate) fn external_change_action(
    content_differs: bool,
    dirty: bool,
    suppressed: bool,
) -> ExternalChange {
    if !content_differs {
        ExternalChange::Ignore
    } else if !dirty {
        ExternalChange::Reload
    } else if suppressed {
        ExternalChange::Ignore
    } else {
        ExternalChange::Toast
    }
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn blank_welcome_requires_no_path_and_untouched_text() {
        assert!(is_blank_welcome(false, "WELCOME", "WELCOME"));
        assert!(!is_blank_welcome(true, "WELCOME", "WELCOME")); // has a file
        assert!(!is_blank_welcome(false, "edited", "WELCOME")); // user typed
    }

    #[test]
    fn save_is_safe_gates_on_content_not_mtime() {
        // QA round-1: content-identical → safe, regardless of what any clock says.
        assert!(save_is_safe("hello", Some("hello")));
        // Any other on-disk content → unsafe, even a single-byte difference
        // that a coarse-resolution mtime clock could never have detected.
        assert!(!save_is_safe("hello", Some("hello!")));
        // Unreadable/deleted on disk → allow (the write itself surfaces any
        // real I/O error; there is nothing there to clobber).
        assert!(save_is_safe("hello", None));
    }

    #[test]
    fn external_change_action_maps_the_full_matrix() {
        use ExternalChange::*;
        // No on-disk change → never act, regardless of dirty/suppressed.
        assert_eq!(external_change_action(false, false, false), Ignore);
        assert_eq!(external_change_action(false, true, false), Ignore);
        // Changed + clean buffer → silent reload (suppressed is irrelevant).
        assert_eq!(external_change_action(true, false, false), Reload);
        assert_eq!(external_change_action(true, false, true), Reload);
        // Changed + dirty buffer → toast, unless the user dismissed the notice.
        assert_eq!(external_change_action(true, true, false), Toast);
        assert_eq!(external_change_action(true, true, true), Ignore);
    }

    #[test]
    fn window_title_is_a_count_at_two_or_more_tabs() {
        assert_eq!(
            window_title_for_tabs(1, Some("foo.md")),
            "foo.md — Scribobulate"
        );
        assert_eq!(window_title_for_tabs(1, None), "Scribobulate");
        assert_eq!(
            window_title_for_tabs(3, Some("foo.md")),
            "3 documents — Scribobulate"
        );
        assert_eq!(window_title_for_tabs(0, None), "Scribobulate"); // transient zero-tab window
    }

    // A fixed sentinel colour so the ordering assertions don't hard-code a
    // particular yellow — the display layer owns "which yellow" (documents.rs).
    const C: &str = "#c";

    fn badge(dirty: bool, pending_external: bool, backing_missing: bool) -> TabBadgeState {
        TabBadgeState {
            dirty,
            pending_external,
            backing_missing,
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
