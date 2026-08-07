//! The window's view mode and split focused-pane, plus the pure `copy_target`
//! decision that ties them together.

/// The window's current view mode: read-only preview, editor-only, or
/// a split editor+preview. Threaded as this typed enum everywhere EXCEPT the
/// `win.view-mode` GAction's own get/set boundary, which is fixed by GVariant to
/// a plain "s"-typed string (see `as_str` / `FromStr` below) — and the on-disk
/// session TOML, which (de)serializes through the same lowercase strings via
/// `serde(rename_all = "lowercase")` so old session files keep parsing.
///
/// Exhaustive `match`es on this enum (no catch-all `_` arm) make a typo'd or a
/// future new mode a compile error instead of silently behaving like `Preview`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ViewMode {
    #[default]
    Preview,
    Edit,
    Split,
}

impl ViewMode {
    /// The `win.view-mode` GVariant / session-TOML string for this mode — the
    /// one place still working in `&str`, because GVariant's "s" type and TOML
    /// both require a plain string there.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            ViewMode::Preview => "preview",
            ViewMode::Edit => "edit",
            ViewMode::Split => "split",
        }
    }

    /// True in the two modes where the editor is visible (edit, split).
    pub(crate) fn is_editor_visible(self) -> bool {
        matches!(self, ViewMode::Edit | ViewMode::Split)
    }

    /// True in the two modes where the preview is visible (preview, split).
    pub(crate) fn is_preview_visible(self) -> bool {
        matches!(self, ViewMode::Preview | ViewMode::Split)
    }
}

impl std::str::FromStr for ViewMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "preview" => Ok(ViewMode::Preview),
            "edit" => Ok(ViewMode::Edit),
            "split" => Ok(ViewMode::Split),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for ViewMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// In split (side-by-side) mode, which of the two panes last genuinely held focus.
/// `win.copy` (enablement + clipboard) and `win.select-all` act on THIS pane so the
/// user copies/selects from the pane they are working in, not always the editor
/// (TDD 9.25). Tracked stickily — updated only when focus lands in a real pane, so a
/// transient surface (the context-menu/menu-bar popover, the find bar) that steals
/// focus can't flip it, exactly like the editor-focus gate (GTK4Rs/AP-20). Irrelevant
/// outside split mode (there is only one visible view), so those modes ignore it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum FocusedPane {
    Editor,
    Preview,
}

/// Which view `win.copy` / `win.select-all` target, as pure data: `None` = the sole
/// visible view (preview-only or edit-only — no ambiguity), `Some(pane)` = that split
/// pane. Only split mode distinguishes panes. This is the decision core of
/// `actions::focused_text_view`; keeping it a pure function pins the split
/// focused-pane fix (TDD 9.25, ScrAP-72) under the coverage gate and unit
/// test, independent of live GTK state.
pub(crate) fn copy_target(mode: ViewMode, focused: FocusedPane) -> Option<FocusedPane> {
    match mode {
        ViewMode::Split => Some(focused),
        ViewMode::Preview | ViewMode::Edit => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::winstate::*;

    #[test]
    fn view_mode_str_round_trips() {
        for m in [ViewMode::Preview, ViewMode::Edit, ViewMode::Split] {
            assert_eq!(m.as_str().parse::<ViewMode>(), Ok(m));
        }
        assert!("bogus".parse::<ViewMode>().is_err());
    }

    #[test]
    fn copy_target_follows_focus_only_in_split() {
        // Non-split: always the sole visible view, regardless of the (stale) focus flag.
        for focused in [FocusedPane::Editor, FocusedPane::Preview] {
            assert_eq!(copy_target(ViewMode::Preview, focused), None);
            assert_eq!(copy_target(ViewMode::Edit, focused), None);
        }
        // Split: whichever pane holds focus — the fix for TDD 9.25 / ScrAP-72.
        assert_eq!(
            copy_target(ViewMode::Split, FocusedPane::Preview),
            Some(FocusedPane::Preview)
        );
        assert_eq!(
            copy_target(ViewMode::Split, FocusedPane::Editor),
            Some(FocusedPane::Editor)
        );
    }
}
