//! Find Next keeps its place when a disclosure is toggled in front of it (TDD 11.12).
//!
//! Its own module, like `bartests` and `splittests`: these drive the preview's fold
//! control through the window, and `window/find.rs` is already the largest file in the
//! tree.

use super::super::testkit::test_app;
use super::testkit::{readout, search};
use super::SearchDir;
use crate::winstate::state;
use gtk::prelude::*;
use gtk::ApplicationWindow;

/// Two matches above a collapsed disclosure that holds two more, then three below it.
/// The summary line carries no match, so the count is the same collapsed or open.
const MD: &str = "note before the fold\n\nnote again before it\n\n<details>\n<summary>Hidden section</summary>\n\nnote hidden one\n\nnote hidden two\n\n</details>\n\nnote in the passage\n\nnote also in the passage\n\nnote at the very end\n";
const TOTAL: i32 = 7;

fn preview_view(win: &ApplicationWindow) -> crate::codeview::CodePreviewView {
    match super::find_target(win) {
        super::FindTarget::Preview(view) => view,
        _ => panic!("preview mode must resolve a preview view"),
    }
}

/// Buffer offset where `needle` starts in the preview as it stands now.
fn offset_of(view: &crate::codeview::CodePreviewView, needle: &str) -> i32 {
    view.buffer()
        .start_iter()
        .forward_search(needle, gtk::TextSearchFlags::empty(), None)
        .unwrap_or_else(|| panic!("the preview must render {needle:?}"))
        .0
        .offset()
}

/// Where the current body match's selection starts, if there is one.
fn selected_at(view: &crate::codeview::CodePreviewView) -> Option<i32> {
    view.buffer()
        .selection_bounds()
        .map(|(start, _)| start.offset())
}

fn step(win: &ApplicationWindow, dir: SearchDir) {
    let st = state(win).expect("an active tab");
    super::find_step(win, &st.search_context, dir);
}

/// **Expanding a block above the current match keeps the reader on that match**
/// (TDD 11.12, MANUAL-TEST 11.12).
///
/// The reader steps backwards onto "6 of 7" (below the folded block, so the step never
/// opens it), then opens the block with its chevron. The splice rewrites the buffer
/// above the match and rebuilds the hit list. The readout must still name the match the
/// reader is on, and Next must go to the one after it. Before the fix the splice's find
/// refresh dropped the cursor: the readout fell to "7 matches" and Next went to "1 of 7"
/// at the top of the document.
///
/// The toggle goes through the control, not the splice, because a click whose handler
/// never reaches the splice changes nothing and looks the same from here.
///
/// Mutation checks: restoring the cursor reset in the boundary refresh
/// (`findbar::refresh_preview_find_highlight` not carrying it) fails the "6 of 7"
/// readout; carrying the cursor as a raw buffer offset instead of a source anchor fails
/// it too, because the expansion moves every offset below the block.
#[gtktest::test]
fn expanding_a_fold_above_the_current_match_keeps_find_next_in_place() {
    let app = test_app("com.extollit.scribobulate.integrationtest.findcursorfold");
    let win = crate::window::new_window(&app, "IT-findcursorfold", MD, None);
    win.change_action_state("view-mode", &"preview".to_variant());
    crate::testpump::drain_for(
        crate::testpump::Clock::Frame,
        std::time::Duration::from_millis(200),
    );
    search(&win, "note");
    assert_eq!(
        readout(&win),
        format!("{TOTAL} matches"),
        "precondition: the hidden matches are counted"
    );

    step(&win, SearchDir::Backward);
    step(&win, SearchDir::Backward);
    let view = preview_view(&win);
    let held = offset_of(&view, "note also in the passage");
    assert_eq!(readout(&win), format!("6 of {TOTAL}"), "precondition");
    assert_eq!(
        selected_at(&view),
        Some(held),
        "precondition: the reader is on the match below the block"
    );

    let generation = view.render_generation();
    let toggle = crate::preview::scrib_render_data(&view)
        .expect("the preview carries render data")
        .borrow()
        .disclosure_lines[0]
        .1
        .clone();
    toggle.set_active(!toggle.is_active());
    crate::testpump::until(
        crate::testpump::Clock::Idle,
        "the disclosure's splice to land",
        || view.render_generation() != generation,
    );
    let view = preview_view(&win);
    assert!(
        offset_of(&view, "note hidden two") < offset_of(&view, "note also in the passage"),
        "precondition: the block is open above the held match"
    );
    assert_ne!(
        offset_of(&view, "note also in the passage"),
        held,
        "precondition: the expansion moved the held match's buffer offset"
    );

    assert_eq!(
        readout(&win),
        format!("6 of {TOTAL}"),
        "the readout must still name the match the reader is on"
    );
    assert_eq!(
        selected_at(&view),
        Some(offset_of(&view, "note also in the passage")),
        "the current match must still be marked where it now sits"
    );

    step(&win, SearchDir::Forward);
    assert_eq!(
        readout(&win),
        format!("{TOTAL} of {TOTAL}"),
        "Find Next must go to the match after the one held"
    );
    assert_eq!(
        selected_at(&view),
        Some(offset_of(&view, "note at the very end")),
        "Find Next must land on the next match in the document"
    );
    win.destroy();
}
