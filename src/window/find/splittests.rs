//! Find in side-by-side mode follows the pane the reader is working in (TDD 11.19).
//!
//! Its own module, like `bartests`, because `window/find.rs` is already the largest
//! file in the tree and these drive the bar and the focus, not the search.

use super::super::testkit::test_app;
use super::testkit::{search, type_query, until_readout};
use super::FindCursor;
use crate::testpump::Clock;
use crate::winstate::{chrome, state, FocusedPane};
use gtk::prelude::*;
use gtk::ApplicationWindow;

/// Three body matches far enough apart that each one is off screen while the
/// previous one is in view, plus a fourth that exists only in the Markdown source (a
/// link target the preview never draws). The fourth is what makes the readout name
/// its pane: the editor counts 4 and the preview counts 3.
fn fixture() -> String {
    let mut md = String::from("# Split find\n\n[a link](https://alpha.example/)\n\n");
    for i in 0..600 {
        if i == 100 || i == 300 || i == 500 {
            md.push_str(&format!("Paragraph {i} holds alpha here.\n\n"));
        } else {
            md.push_str(&format!("Paragraph {i} is filler.\n\n"));
        }
    }
    md
}
const EDITOR_COUNT: i32 = 4;
const PREVIEW_COUNT: i32 = 3;

/// A presented split window over the fixture, laid out: both panes have a viewport.
fn split_window(suffix: &str) -> ApplicationWindow {
    let app = test_app(&format!(
        "com.extollit.scribobulate.integrationtest.splitfind{suffix}"
    ));
    let win = crate::window::new_window(&app, "IT-splitfind", &fixture(), None);
    crate::window::change_action_state(&win, "view-mode", &"split".to_variant());
    win.set_default_size(1000, 600);
    win.present();
    let editor = state(&win).expect("active tab").editor.clone();
    let view = preview_view(&win);
    crate::testpump::until(
        crate::testpump::Clock::Frame,
        "both split panes to be laid out",
        || editor.visible_rect().height() > 0 && view.visible_rect().height() > 0,
    );
    win
}

fn preview_view(win: &ApplicationWindow) -> crate::codeview::CodePreviewView {
    crate::window::zoom::get_preview_view(win).expect("a preview view in split mode")
}

/// Give `widget` the keyboard focus as a click would, from a cleared focus so the
/// grab is a real focus change the gate is notified of, and return once the window
/// records `pane` as the focused one. What the focus change retargets (the readout,
/// the highlights) is waited on by the caller, through the readout.
fn focus(win: &ApplicationWindow, widget: &impl IsA<gtk::Widget>, pane: FocusedPane) {
    GtkWindowExt::set_focus(win, None::<&gtk::Widget>);
    widget.grab_focus();
    let ch = chrome(win).expect("window chrome");
    crate::testpump::until(
        crate::testpump::Clock::Idle,
        &format!("the window to record {pane:?} as the focused pane"),
        || ch.focused_pane.get() == pane,
    );
}

fn preview_highlighted(view: &crate::codeview::CodePreviewView) -> bool {
    let buf = view.buffer();
    let Some(tag) = buf.tag_table().lookup(super::PREVIEW_HL_TAG) else {
        return false;
    };
    let mut it = buf.start_iter();
    it.starts_tag(Some(&tag)) || it.forward_to_tag_toggle(Some(&tag))
}

/// Whether the preview line holding buffer offset `at` is inside the viewport.
fn on_screen(view: &crate::codeview::CodePreviewView, at: i32) -> bool {
    let (y, h) = view.line_yrange(&view.buffer().iter_at_offset(at));
    let rect = view.visible_rect();
    rect.height() > 0 && y >= rect.y() && y + h <= rect.y() + rect.height()
}

/// TDD 11.19: with the preview pane focused in split mode, Find searches the PREVIEW,
/// and every Next scrolls the preview to the match it lands on.
///
/// Both halves were broken together. The target was decided from the view mode alone,
/// so split always searched the editor: the readout counted the editor's 4 against
/// the preview's 3, and the cursor indexed the editor's list. And each step's editor
/// scroll was undone on the next frame by the split scroll sync, which takes its lead
/// from the focused pane — the preview — and projected the unmoved preview back onto
/// the editor, so neither pane moved.
///
/// Mutation check: making `find_target` decide from the view mode alone again
/// (`if current_mode(window) != ViewMode::Preview { return FindTarget::Editor; }`)
/// fails here on the readout ("4 matches" for the preview's 3). Dropping the scroll from
/// `land_on_hit` (fix in place) fails here on the preview never reaching the match, so
/// the scroll half is guarded on its own.
#[gtktest::test]
fn find_in_split_searches_and_scrolls_the_focused_preview() {
    let win = split_window("preview");
    let view = preview_view(&win);
    focus(&win, &view, FocusedPane::Preview);
    let ch = chrome(&win).expect("window chrome");

    search(&win, "alpha");
    assert_eq!(
        ch.focused_pane.get(),
        FocusedPane::Preview,
        "precondition: the find field taking focus leaves the preview the focused pane"
    );
    until_readout(
        &win,
        &readout_for_total(PREVIEW_COUNT),
        &format!("the readout to count the preview's matches, not the editor's {EDITOR_COUNT}"),
    );
    assert!(
        preview_highlighted(&view),
        "the preview carries the match highlights"
    );
    let st = state(&win).expect("active tab");
    assert!(
        !st.search_context.is_highlight(),
        "the editor, the pane not searched, keeps no find highlight"
    );

    let mut landed = Vec::new();
    for step in 1..=PREVIEW_COUNT {
        // What the Next button and Enter both run.
        super::find_step(&win, &st.search_context, super::SearchDir::Forward);
        let FindCursor::Preview { ordinal, at } = st.find_cursor.get() else {
            panic!(
                "step {step}: Next stepped the editor's list ({:?}) while the reader is in \
                 the preview",
                st.find_cursor.get()
            );
        };
        assert_eq!(ordinal, step, "Next advances one match at a time");
        crate::testpump::until_for(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_secs(5),
            &format!("the preview to scroll match {step} (offset {at}) into view"),
            || on_screen(&view, at),
        );
        landed.push(at);
    }
    assert!(
        landed.windows(2).all(|w| w[0] < w[1]),
        "each step lands further down the document: {landed:?}"
    );
    assert!(
        !on_screen(&view, landed[0]),
        "precondition: the matches are far enough apart that reaching the last one \
         needed a real scroll away from the first"
    );
    assert!(
        st.editor_buf.selection_bounds().is_none(),
        "the editor's selection is untouched while the preview is searched"
    );
    win.destroy();
}

/// The readout `"N of M"` before any step is just the total — written through the
/// app's own formatter so this test does not pin its wording.
fn readout_for_total(total: i32) -> String {
    let probe = gtk::Label::new(None);
    super::set_match_label(&probe, 0, total);
    probe.text().to_string()
}

/// TDD 11.19: moving the focus to the other split pane while the bar is open moves
/// the search with it — readout, highlights, and the replace row's availability.
///
/// Mutation check: removing the focus gate's call to
/// `findbar::retarget_find_to_focused_pane` fails here — the readout stays on the
/// pane the search began in until the query is next edited.
#[gtktest::test]
fn moving_focus_between_split_panes_moves_the_open_search() {
    let win = split_window("retarget");
    let view = preview_view(&win);
    let st = state(&win).expect("active tab");
    focus(&win, &st.editor, FocusedPane::Editor);
    crate::window::actions::simple_action(&win, "find-replace")
        .expect("win.find-replace is registered")
        .activate(None);
    type_query(&win, "alpha");
    until_readout(
        &win,
        &readout_for_total(EDITOR_COUNT),
        "the editor's engine to finish counting",
    );
    assert!(
        !preview_highlighted(&view),
        "the editor search leaves the preview bare"
    );
    assert!(
        st.chrome().replace_row.is_sensitive(),
        "replace works in the editor"
    );

    focus(&win, &view, FocusedPane::Preview);
    until_readout(
        &win,
        &readout_for_total(PREVIEW_COUNT),
        "focusing the preview to re-count in the preview",
    );
    assert!(preview_highlighted(&view), "the preview is now highlighted");
    assert!(
        !st.search_context.is_highlight(),
        "the editor left behind keeps no engine highlight"
    );
    assert!(
        !st.chrome().replace_row.is_sensitive(),
        "the preview is read-only, so replace is unavailable while it is searched"
    );

    focus(&win, &st.editor, FocusedPane::Editor);
    until_readout(
        &win,
        &readout_for_total(EDITOR_COUNT),
        "the readout to return to the editor's count",
    );
    assert!(
        !preview_highlighted(&view),
        "the preview keeps no highlight once the search has moved back to the editor"
    );
    assert!(
        st.search_context.is_highlight(),
        "the editor searched again is lit again"
    );
    assert!(st.chrome().replace_row.is_sensitive());
    win.destroy();
}

/// TDD 11.19: REOPENING the bar over an unchanged query describes the pane now
/// searched. Close a preview search, focus the editor, reopen: the readout is the
/// editor's count and the editor is lit. Then the reverse: close, focus the preview,
/// reopen — the editor is unlit.
///
/// Reported by the macOS seat: the readout kept the preview's count after a reopen from
/// the editor, because an unchanged query leaves the engine's count notification
/// silent. Mutation check: restoring the scoped-only readout refresh in
/// `open_find_bar` fails the first readout assertion; restoring the unconditional
/// `set_highlight(unscoped)` there fails the last.
#[gtktest::test]
fn reopening_the_bar_in_split_describes_the_pane_now_searched() {
    let win = split_window("reopen");
    let view = preview_view(&win);
    let st = state(&win).expect("active tab");
    let close = |w: &ApplicationWindow| {
        state(w)
            .expect("active tab")
            .chrome()
            .find_entry
            .emit_by_name::<()>("stop-search", &[]);
        let bar = state(w)
            .expect("active tab")
            .chrome()
            .find_bar_revealer
            .clone();
        crate::testpump::until(Clock::Idle, "the find bar to close", || {
            !bar.reveals_child()
        });
    };
    let reopen = |w: &ApplicationWindow| {
        crate::window::actions::simple_action(w, "find")
            .expect("win.find is registered")
            .activate(None);
        let bar = state(w)
            .expect("active tab")
            .chrome()
            .find_bar_revealer
            .clone();
        crate::testpump::until(Clock::Idle, "the find bar to reopen", || {
            bar.reveals_child()
        });
    };

    focus(&win, &view, FocusedPane::Preview);
    search(&win, "alpha");
    until_readout(
        &win,
        &readout_for_total(PREVIEW_COUNT),
        "precondition: the preview search to count the preview",
    );
    close(&win);

    focus(&win, &st.editor, FocusedPane::Editor);
    reopen(&win);
    until_readout(
        &win,
        &readout_for_total(EDITOR_COUNT),
        "reopened from the editor, the readout to be the editor's count",
    );
    assert!(
        st.search_context.is_highlight(),
        "reopened from the editor, the editor is lit"
    );
    close(&win);

    focus(&win, &view, FocusedPane::Preview);
    reopen(&win);
    until_readout(
        &win,
        &readout_for_total(PREVIEW_COUNT),
        "reopened from the preview, the readout to be the preview's count",
    );
    assert!(
        !st.search_context.is_highlight(),
        "reopened from the preview, the editor keeps no highlight"
    );
    win.destroy();
}

/// TDD 11.19 with the lead elsewhere: a wheel over the editor makes it lead the split
/// sync without moving the focus, so a Find from the focused preview must take the lead
/// back before scrolling, or the next sync frame puts the editor's position back and
/// scrolls the match away (ScrAP-361).
///
/// Mutation check: dropping the `panescroll::lead` call from `scroll_to_preview_hit`
/// fails the driver assertion here.
#[gtktest::test]
fn find_in_the_preview_takes_the_lead_from_a_wheeled_editor() {
    let win = split_window("preview_lead");
    let view = preview_view(&win);
    focus(&win, &view, FocusedPane::Preview);
    let st = state(&win).expect("active tab");
    // What a wheel over the editor does: lead without taking focus.
    st.scroll.driver.set(crate::winstate::ScrollDriver::Editor);

    search(&win, "alpha");
    super::find_step(&win, &st.search_context, super::SearchDir::Forward);
    let FindCursor::Preview { at, .. } = st.find_cursor.get() else {
        panic!("precondition: Find stepped the focused preview");
    };
    assert_eq!(
        st.scroll.driver.get(),
        crate::winstate::ScrollDriver::Preview,
        "the preview's match scroll must lead the split sync"
    );
    crate::testpump::until_for(
        crate::testpump::Clock::Frame,
        std::time::Duration::from_secs(5),
        "the match to stay in view after the sync has run",
        || on_screen(&view, at),
    );
    win.destroy();
}
