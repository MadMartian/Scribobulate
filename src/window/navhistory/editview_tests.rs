//! TDD 23.15 — Back and Forward in the editor-only view, and entries honoured in a
//! view other than the one that recorded them.
//!
//! Unlike the preview, the editor reports its top line from its real viewport
//! (`ViewportTopIter`), so these bodies present a sized window and pump frames until
//! each scroll has landed, rather than reading a tracked target.

use super::testkit::*;
use super::*;
use crate::outline::{build_tree, expansion::paths_in_document_order, extract_headings};
use crate::winstate::{state, ViewMode};

/// Sections in [`doc`], and the source lines each costs: heading, blank, four lines
/// of prose, blank.
const SECTIONS: i32 = 40;
const LINES_PER_SECTION: i32 = 7;

fn doc() -> String {
    let mut md = String::new();
    for n in 1..=SECTIONS {
        md.push_str(&format!("## Section {n}\n\n"));
        for _ in 0..4 {
            md.push_str(&format!("Prose for section {n}.\\\n"));
        }
        md.push('\n');
    }
    md
}

/// The source line `## Section n` is on.
fn heading_line(n: i32) -> i32 {
    (n - 1) * LINES_PER_SECTION
}

/// A presented window on [`doc`] in `mode`, with its editor laid out to a real
/// scroll range.
fn windowed(app_id: &str, mode: &str) -> (ApplicationWindow, Rc<TabState>) {
    let app = make_app(app_id);
    let window = new_window(&app, "IT", &doc(), None);
    window.set_default_size(800, 600);
    window.present();
    change_action_state(&window, "view-mode", &mode.to_variant());
    let st = state(&window).expect("a tab");
    let scroller = if mode == "edit" {
        st.split.editor_scroller()
    } else {
        st.split
            .preview_scroller()
            .expect("the mode shows a preview")
    };
    crate::testpump::until(
        crate::testpump::Clock::Frame,
        "the pane to report a scrollable range",
        || {
            let adj = scroller.vadjustment();
            adj.page_size() > 0.0 && adj.upper() > adj.page_size() * 2.0
        },
    );
    (window, st)
}

fn editor_top(st: &TabState) -> i32 {
    let top = crate::saferizer::viewport::ViewportTopIter::top_offset(&st.editor);
    st.editor_buf.iter_at_offset(top).line()
}

fn caret(st: &TabState) -> (i32, i32) {
    let it = st.editor_buf.iter_at_mark(&st.editor_buf.get_insert());
    (it.line(), it.line_offset())
}

fn until_editor_top(st: &TabState, line: i32, what: &str) {
    crate::testpump::until(crate::testpump::Clock::Frame, what, || {
        editor_top(st) == line
    });
}

/// Put the reader at `top` with the caret at `caret`, the way scrolling and clicking do.
fn park_editor(st: &TabState, top: i32, caret: (i32, i32)) {
    let it = st
        .editor_buf
        .iter_at_line_offset(caret.0, caret.1)
        .expect("the caret position exists");
    st.editor_buf.place_cursor(&it);
    crate::window::panescroll::editor_to_line(st, top);
    until_editor_top(st, top, "the editor to settle where the reader parked it");
}

/// Activate the outline row for `## Section n`, as a click on it does.
fn activate_section(window: &ApplicationWindow, n: i32) {
    let headings = extract_headings(&doc());
    let paths = paths_in_document_order(&build_tree(&headings));
    let index = (n - 1) as usize;
    crate::window::outline_nav::navigate_to_heading(window, &paths[index], index);
}

/// The whole loop in the editor-only view: an outline jump records, Back restores the
/// viewport AND the caret, Forward returns to the heading, and re-activating the
/// heading the reader is on adds no entry.
///
/// The caret half is the editor's own: the jump moved it, so a Back that restored only
/// the viewport would leave it, and the outline's highlight, on the heading.
#[gtktest::test]
fn an_outline_jump_in_the_editor_only_view_walks_back_and_forward() {
    let (window, st) = windowed(
        "com.extollit.scribobulate.integrationtest.nav.editjump",
        "edit",
    );
    assert_eq!(st.view_mode.get(), ViewMode::Edit, "precondition");
    assert!(
        !enabled(&window, "nav-back"),
        "precondition: no history yet"
    );

    let (departure, held) = (heading_line(6), (heading_line(6) + 3, 4));
    park_editor(&st, departure, held);
    let target = heading_line(25);
    activate_section(&window, 25);
    until_editor_top(&st, target, "the jump to land on the heading");
    assert_eq!(
        caret(&st).0,
        target,
        "the jump moves the caret to the heading"
    );
    assert!(enabled(&window, "nav-back"), "the jump is a navigation");

    WidgetExt::activate_action(&window, "win.nav-back", None).expect("activates");
    until_editor_top(&st, departure, "Back to land where the reader left from");
    assert_eq!(caret(&st), held, "Back puts the caret back too");
    assert!(enabled(&window, "nav-forward"));

    WidgetExt::activate_action(&window, "win.nav-forward", None).expect("activates");
    until_editor_top(&st, target, "Forward to land on the heading");
    assert_eq!(caret(&st).0, target);

    activate_section(&window, 25);
    WidgetExt::activate_action(&window, "win.nav-back", None).expect("activates");
    until_editor_top(
        &st,
        departure,
        "one Back to leave a heading re-activated in place, not two",
    );

    window.destroy();
}

/// GEP-53 in the editor: a departure from the very top of the document is a place to
/// return to, not "already at the top, nothing to do".
#[gtktest::test]
fn back_from_the_top_of_the_document_in_the_editor_returns_to_the_top() {
    let (window, st) = windowed(
        "com.extollit.scribobulate.integrationtest.nav.edittop",
        "edit",
    );
    park_editor(&st, 0, (0, 0));
    activate_section(&window, 25);
    until_editor_top(&st, heading_line(25), "the jump to land");

    WidgetExt::activate_action(&window, "win.nav-back", None).expect("activates");
    until_editor_top(&st, 0, "Back to scroll back up to the top");
    assert_eq!(caret(&st), (0, 0));

    window.destroy();
}

/// An entry recorded in the editor-only view is honoured in the preview: Back opens
/// the preview at the place the reader left the editor from.
#[gtktest::test]
fn an_editor_entry_is_honoured_in_the_preview() {
    let (window, st) = windowed(
        "com.extollit.scribobulate.integrationtest.nav.editthenpreview",
        "edit",
    );
    park_editor(&st, heading_line(6), (heading_line(6), 0));
    activate_section(&window, 25);
    until_editor_top(&st, heading_line(25), "the jump to land");

    change_action_state(&window, "view-mode", &"preview".to_variant());
    let (sw, view) = preview_of(&window);
    crate::testpump::until(
        crate::testpump::Clock::Frame,
        "the preview to render",
        || crate::preview::preview_heading_line(&sw, "section-6").is_some(),
    );
    WidgetExt::activate_action(&window, "win.nav-back", None).expect("activates");
    crate::testpump::until(
        crate::testpump::Clock::Frame,
        "Back to open the preview on the section the editor was left from",
        || view.reading_line() == line_of(&sw, "section-6"),
    );

    window.destroy();
}

/// …and the converse: an entry recorded in the preview is honoured in the editor-only
/// view, through the source position captured while the preview still existed.
#[gtktest::test]
fn a_preview_entry_is_honoured_in_the_editor() {
    let (window, st) = windowed(
        "com.extollit.scribobulate.integrationtest.nav.previewthenedit",
        "preview",
    );
    let (sw, view) = preview_of(&window);
    crate::testpump::until(
        crate::testpump::Clock::Frame,
        "the preview to render",
        || crate::preview::preview_heading_line(&sw, "section-6").is_some(),
    );
    park_reader_at(&view, line_of(&sw, "section-6"));
    activate_section(&window, 25);
    assert!(
        enabled(&window, "nav-back"),
        "the preview jump is a navigation"
    );

    change_action_state(&window, "view-mode", &"edit".to_variant());
    WidgetExt::activate_action(&window, "win.nav-back", None).expect("activates");
    until_editor_top(
        &st,
        heading_line(6),
        "Back to open the editor on the section the preview was left from",
    );

    window.destroy();
}

/// TDD 23.14 in the editor-only view: with no preview to ask, the source's own
/// headings decide which entries are stale. The shape of the preview's own test
/// (`headings_a_reload_removed_stop_being_stops`): two jumps, the second made without
/// moving off the first, then both headings deleted in the editor. One Back press
/// reaches the place the reader started from, and nothing is left behind it.
///
/// Mutation: answering every slug as resolved in the editor-only branch of
/// `reconcile_nav_history_headings` leaves Back enabled at the end.
#[gtktest::test]
fn headings_deleted_in_the_editor_stop_being_stops() {
    let (window, st) = windowed(
        "com.extollit.scribobulate.integrationtest.nav.editstale",
        "edit",
    );
    let departure = heading_line(6);
    park_editor(&st, departure, (departure, 0));
    activate_section(&window, 25);
    until_editor_top(&st, heading_line(25), "the first jump to land");
    activate_section(&window, 30);
    until_editor_top(&st, heading_line(30), "the second jump to land");

    let buf = &st.editor_buf;
    for n in [30, 25] {
        let mut start = buf.iter_at_line(heading_line(n)).expect("the heading line");
        let mut end = start;
        end.forward_line();
        buf.delete(&mut start, &mut end);
    }
    refresh_nav_history_actions(&window);

    WidgetExt::activate_action(&window, "win.nav-back", None).expect("activates");
    until_editor_top(
        &st,
        departure,
        "one Back to reach the place the reader started from",
    );
    assert!(
        !enabled(&window, "nav-back"),
        "nothing is behind the departure once both headings are gone"
    );

    window.destroy();
}
