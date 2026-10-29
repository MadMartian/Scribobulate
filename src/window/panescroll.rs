//! `panescroll`: the one way a command scrolls a text pane on purpose.
//!
//! In split mode the two panes are tied by the scroll sync (`window::scrollsync`), which
//! projects the LEADING pane onto the other every frame. A command that scrolls the
//! pane that is not leading has its scroll cancelled on the next frame: the sync puts
//! the leader's unchanged position straight back. Nothing warns; the command simply
//! appears to do nothing. That happened to Find, then again to Undo/Redo (ScrAP-361),
//! each time because one call site scrolled without first making its pane lead.
//!
//! So a navigation names the pane it moves, and this module makes that pane lead
//! before scrolling it:
//!
//! - [`editor_to_mark`] is the only route for scrolling the editor to a mark. The raw
//!   `farscroll::scroll_to_mark_when_ready` is banned in `clippy.toml`; this module's
//!   call carries the sanctioned `#[expect]`.
//! - [`preview_to_fragment`] is the only route for scrolling the preview to a heading
//!   named by a link fragment. The raw `preview::scroll_preview_to_fragment` is banned
//!   in `clippy.toml`; this module's call carries the sanctioned `#[expect]`.
//! - [`preview_to_cell`] is the only route for scrolling the preview to a table cell.
//!   The raw `CodePreviewView::scroll_to_cell_offset` is banned in `clippy.toml`; this
//!   module's call carries the sanctioned `#[expect]`.
//! - [`preview_to_offset`] is the route for a navigation that scrolls the preview to a
//!   buffer offset (Find's hits, the reveal of a change). The raw
//!   `CodePreviewView::scroll_to_buffer_offset` is NOT banned, and deliberately: most of
//!   its callers are not navigations at all — the preview's own coalesced scroll inside
//!   the view, the heading and fragment seams, the reading-position restores, and a link
//!   click in the preview, which the click itself has already made lead — so a ban would
//!   be almost all exceptions (POLICY § Typed GTK seams).
//! - [`lead`] is for a navigation whose scroll goes through another seam (the preview's
//!   heading and line scrolls), which takes the pane and has no mark.
//!
//! Outside split mode the lead has no effect: only the split sync reads it.
//!
//! What this module does NOT cover: restoring a reading position after a re-render or
//! a mode switch. That is preserving a place, not moving to a new one, and it holds the
//! sync off by other means (Reading-Position Preservation CAM).

use gtk::prelude::*;

use crate::codeview::CodePreviewView;
use crate::preview::scrib_render_data;
use crate::readingpos::{self, DocPosition};
use crate::winstate::{ScrollDriver, TabState};

/// Make `pane` the one the split scroll sync follows, so a scroll the caller is about
/// to issue on it sticks and the other pane follows it.
pub(crate) fn lead(st: &TabState, pane: ScrollDriver) {
    st.scroll.driver.set(pane);
}

/// Scroll the preview to the heading `fragment` names, with the preview leading the
/// split sync. `false` when the tab has no preview (Edit mode) or no such heading.
pub(crate) fn preview_to_fragment(st: &TabState, fragment: &str) -> bool {
    let Some(sw) = st.split.preview_scroller() else {
        return false;
    };
    lead(st, ScrollDriver::Preview);
    #[expect(
        clippy::disallowed_methods,
        reason = "the sanctioned route: the preview was made to lead the split sync on the line above (ScrAP-361)"
    )]
    crate::preview::scroll_preview_to_fragment(&sw, fragment)
}

/// Scroll the preview `view` to buffer `offset`, with the preview leading the split sync
/// (ScrAP-361).
pub(crate) fn preview_to_offset(st: &TabState, view: &CodePreviewView, offset: i32) {
    lead(st, ScrollDriver::Preview);
    view.scroll_to_buffer_offset(offset);
}

/// Scroll the preview `view` to the row of the table cell `label` anchored at
/// `anchor_off`, with the preview leading the split sync (ScrAP-361).
pub(crate) fn preview_to_cell(
    st: &TabState,
    view: &CodePreviewView,
    anchor_off: i32,
    label: &gtk::Label,
) {
    lead(st, ScrollDriver::Preview);
    #[expect(
        clippy::disallowed_methods,
        reason = "the sanctioned route: the preview was made to lead the split sync on the line above (ScrAP-361)"
    )]
    view.scroll_to_cell_offset(anchor_off, label);
}

/// How [`editor_to_mark`] places the mark: the same four arguments as
/// `gtk_text_view_scroll_to_mark`, named.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Placement {
    pub(crate) within_margin: f64,
    pub(crate) use_align: bool,
    pub(crate) xalign: f64,
    pub(crate) yalign: f64,
}

impl Placement {
    /// The least movement that brings the mark on screen; nothing moves when it
    /// already is.
    pub(crate) const MINIMAL: Self = Self {
        within_margin: 0.0,
        use_align: false,
        xalign: 0.0,
        yalign: 0.0,
    };
    /// The mark's line at the top of the viewport.
    pub(crate) const TOP: Self = Self {
        within_margin: 0.0,
        use_align: true,
        xalign: 0.0,
        yalign: 0.0,
    };
    /// The mark's line at the middle of the viewport.
    pub(crate) const MIDDLE: Self = Self {
        within_margin: 0.0,
        use_align: true,
        xalign: 0.0,
        yalign: 0.5,
    };
}

/// Scroll the editor so `mark` is placed as `placement` says, with the editor leading
/// the split sync. Reaches marks far into a document still being laid out
/// (`farscroll`, GTK4Rs/AP-260).
pub(crate) fn editor_to_mark(st: &TabState, mark: &gtk::TextMark, placement: Placement) {
    lead(st, ScrollDriver::Editor);
    #[expect(
        clippy::disallowed_methods,
        reason = "the sanctioned route: the editor was made to lead the split sync on the line above (ScrAP-361)"
    )]
    crate::farscroll::scroll_to_mark_when_ready(
        st.editor.upcast_ref(),
        mark,
        placement.within_margin,
        placement.use_align,
        placement.xalign,
        placement.yalign,
    );
}

/// The preview pane's view, when the tab has one.
fn preview_view(st: &TabState) -> Option<CodePreviewView> {
    st.split
        .preview_scroller()?
        .child()?
        .downcast::<CodePreviewView>()
        .ok()
}

/// The first and last preview buffer lines on screen, read before a change re-renders
/// the preview so [`preview_reveal`] can tell whether the change was already in view.
/// `None` when the preview is not on screen.
pub(crate) fn preview_lines_on_screen(st: &TabState) -> Option<(i32, i32)> {
    let view = preview_view(st)?;
    // An unmapped view answers `line_at_y` with the buffer's last line (GTK4Rs/AP-263).
    if !view.is_mapped() {
        return None;
    }
    let rect = view.visible_rect();
    let (top, _) = view.line_at_y(rect.y());
    let (bottom, _) = view.line_at_y(rect.y() + rect.height() - 1);
    Some((top.line(), bottom.line()))
}

/// Bring `pos` into view in the preview, with the preview leading the split sync,
/// unless its line was already on screen in `before` (from [`preview_lines_on_screen`]).
///
/// The line lands at the top of the viewport. The preview's own coalesced scroll is
/// used, so this supersedes a re-render's pending restore of the old reading position
/// when it is issued after that re-render. `before` describes the buffer as it was
/// BEFORE the re-render, so the "already on screen" test is approximate by however
/// many lines the change itself added or removed above the viewport.
pub(crate) fn preview_reveal(st: &TabState, pos: DocPosition, before: Option<(i32, i32)>) {
    let Some(view) = preview_view(st) else { return };
    let Some(rd) = scrib_render_data(&view) else {
        return;
    };
    let buf_char = {
        let rd = rd.borrow();
        readingpos::reading_to_preview_char(&rd.source_map_inv, &rd.shifts, &rd.original_owned, pos)
    };
    let line = view.buffer().iter_at_offset(buf_char).line();
    if !reveal_needed(before, line) {
        return;
    }
    preview_to_offset(st, &view, buf_char);
}

/// Whether [`preview_reveal`] must scroll to bring preview `line` into view, given the
/// lines `before` that were on screen (inclusive at both ends). With no `before` the
/// preview was not on screen, and a reveal is always due.
fn reveal_needed(before: Option<(i32, i32)>, line: i32) -> bool {
    !before.is_some_and(|(top, bottom)| (top..=bottom).contains(&line))
}

#[cfg(test)]
mod tests {
    use super::reveal_needed;

    /// TDD 9.16: a change on either edge of the viewport is on screen and needs no
    /// scroll; one line past either edge does; a preview that was not on screen always
    /// does.
    #[test]
    fn a_reveal_is_needed_only_for_a_line_off_the_screen_before() {
        let on = Some((10, 30));
        assert!(!reveal_needed(on, 10), "the top line is on screen");
        assert!(!reveal_needed(on, 20), "a middle line is on screen");
        assert!(!reveal_needed(on, 30), "the bottom line is on screen");
        assert!(reveal_needed(on, 9), "the line above the top is not");
        assert!(reveal_needed(on, 31), "the line below the bottom is not");
        assert!(
            reveal_needed(None, 20),
            "no preview on screen: always reveal"
        );
    }
}
