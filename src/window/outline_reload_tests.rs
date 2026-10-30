//! TDD 3.2a (and 12.16) across a LIVE reload: the reader's place, and the outline row that
//! names it, both come back — in every view mode, with the outline filtered or not.
//!
//! Two defects answered this one report, one per kind of pane:
//!
//! * **Edit and Split** put the editor caret on the first line (`load_into_editor`) and
//!   left the editor's viewport at the top, and the outline follows the caret in those
//!   modes, so it moved to its top entry and stayed there.
//! * **Preview** restored the reader's line into the adjustment as its buffer y, which is
//!   one `top_margin` short of the value that shows it at the top
//!   (`ViewportTopIter::value_for_top_y`). A reader parked ON a heading — where any
//!   outline navigation leaves them — came back with the last line of the previous
//!   section at the top, and the scroll-spy moved the highlight back a section; under a
//!   filter hiding that section, to its nearest shown ancestor.
//!
//! The filter is not needed for either; it only changes which row the wrong section
//! falls back to. Cases run both ways regardless, because the report came from a
//! filtered outline.
//!
//! What is asserted is what the reader sees: the outline's selected row, the line at the
//! top of the reading pane, and in the editor modes the caret — never the restore's own
//! bookkeeping, which was right all along in the preview case.
//!
//! The last two cases share this fixture for TDD 7.5's split sync: a reader parked ON a
//! heading in Split, with the editor driving the preview (the projection every live
//! reload and live-preview re-render runs), must keep the heading at the preview's top.

use super::outline_nav::outline_tree_model;
use super::*;
use crate::outline_view::HeadingObject;
use crate::testpump::{drain_for, until, until_stable, Clock};
use std::time::Duration;

/// The heading the reader parks on, and the filter query that narrows the outline to it.
const TARGET: &str = "Target heading";
const QUERY: &str = "target";

/// Sixty sections of a few paragraphs each — far enough down that the reload's
/// progressive restore has heights to validate above the target — with the target
/// two-thirds of the way down, its predecessor's section filtered out by [`QUERY`].
fn document() -> String {
    let mut s = String::from("# Book\n\n");
    for chapter in 0..6 {
        s.push_str(&format!("## Chapter {chapter}\n\n"));
        for section in 0..10 {
            let title = if chapter == 4 && section == 3 {
                TARGET.to_owned()
            } else {
                format!("Section {chapter}.{section}")
            };
            s.push_str(&format!("### {title}\n\n"));
            for para in 0..4 {
                s.push_str(&format!(
                    "Paragraph {para} of {chapter}.{section}, long enough to wrap onto a \
                     second display row in a pane of ordinary width, so that line heights \
                     differ from a bare line count.\n\n"
                ));
            }
        }
    }
    s
}

fn target_line(doc: &str) -> i32 {
    doc.lines()
        .position(|l| l == format!("### {TARGET}"))
        .expect("the fixture carries the target heading") as i32
}

fn open(suffix: &str, mode: &str, doc: &str) -> ApplicationWindow {
    let app = crate::window::testkit::test_app_suffixed(suffix);
    let window = crate::window::new_window(&app, "IT", doc, None);
    window.set_default_size(1000, 700);
    window.present();
    until(
        Clock::Idle,
        "the window to map with its outline built",
        || window.is_mapped() && outline_tree_model(&window).is_some(),
    );
    window.change_action_state("view-mode", &mode.to_variant());
    drain_for(Clock::Frame, Duration::from_millis(300));
    assert_eq!(
        current_mode(&window).as_str(),
        mode,
        "precondition: the view mode took"
    );
    window
}

/// The title of the outline's selected row, if any.
fn selected_title(window: &ApplicationWindow) -> Option<String> {
    let st = state(window)?;
    super::sidebar::list_view_of(&st.chrome().outline_scroller)
        .and_then(|lv| lv.model())
        .and_then(|m| m.downcast::<gtk::SingleSelection>().ok())
        .and_then(|s| s.selected_item())
        .and_downcast::<gtk::TreeListRow>()
        .and_then(|r| r.item())
        .and_downcast::<HeadingObject>()
        .and_then(|h| h.path().last().map(|step| step.title.clone()))
}

fn filter_outline(window: &ApplicationWindow) {
    let st = state(window).expect("tab");
    let ch = st.chrome();
    window
        .lookup_action(SidebarPaneKind::Outline.filter_action())
        .expect("the filter command is registered")
        .activate(None);
    until(Clock::Idle, "the filter bar to open", || {
        ch.outline_filter.bar.is_search_mode()
    });
    ch.outline_filter.entry.set_text(QUERY);
    until(
        Clock::Frame,
        "the filter text to reach the document",
        || st.outline_filter.borrow().text == QUERY,
    );
}

/// The line at the top of the pane the reader is reading in this mode.
fn reading_pane_top(window: &ApplicationWindow) -> i32 {
    let st = state(window).expect("tab");
    if current_mode(window) == ViewMode::Preview {
        let view = st
            .split
            .preview_scroller()
            .and_then(|sw| sw.child())
            .and_then(|c| c.downcast::<crate::codeview::CodePreviewView>().ok())
            .expect("a preview view");
        crate::saferizer::viewport::ViewportTopIter::of(&view).line()
    } else {
        crate::saferizer::viewport::ViewportTopIter::of(&st.editor).line()
    }
}

fn caret_line(window: &ApplicationWindow) -> i32 {
    let st = state(window).expect("tab");
    st.editor_buf
        .iter_at_mark(&st.editor_buf.get_insert())
        .line()
}

/// Wait for the reload's restore and the scroll-spy to stop moving, and report where
/// they left the outline, the reading pane and the caret.
fn settled(window: &ApplicationWindow) -> (Option<String>, i32, i32) {
    let s = until_stable(
        Clock::Frame,
        Duration::from_secs(15),
        Duration::from_millis(800),
        || {
            (
                selected_title(window),
                reading_pane_top(window),
                caret_line(window),
            )
        },
    );
    assert!(s.converged, "the restore never settled");
    s.value.expect("sampled at least once")
}

/// What every reload in this file appends to the document.
const APPENDED: &str = "appended by another process";

/// Another process appends [`APPENDED`] to the file; the monitor's decision core hands
/// the new text to this same entry point.
fn reload_with_appended(window: &ApplicationWindow, doc: &str) {
    super::reload::apply_external_reload(window, &format!("{doc}\n{APPENDED}\n"));
}

/// The reload reached the panes: the editor holds the appended text and, where a
/// preview is shown (`mode` is not `"edit"`), so does the preview. Every other assertion
/// in this file is "the same as before", which is also what a reload that did nothing
/// would leave, so each reload case asserts this first.
fn assert_reloaded(window: &ApplicationWindow, mode: &str) {
    let st = state(window).expect("tab");
    assert!(
        st.editor_text().contains(APPENDED),
        "{mode}: the reload reached the editor"
    );
    if mode == "edit" {
        return;
    }
    let preview = st
        .split
        .preview_scroller()
        .and_then(|sw| sw.child())
        .and_then(|c| c.downcast::<crate::codeview::CodePreviewView>().ok())
        .expect("a preview view");
    let buf = preview.buffer();
    assert!(
        buf.slice(&buf.start_iter(), &buf.end_iter(), false)
            .contains(APPENDED),
        "{mode}: the reload re-rendered the preview"
    );
}

/// Park the reader on the target the way the report did: the filter box's Enter goes to
/// the first match. Unfiltered, the filter is then closed — the place stays, the outline
/// is whole again.
fn park_in_preview(window: &ApplicationWindow, filtered: bool) {
    filter_outline(window);
    state(window)
        .expect("tab")
        .chrome()
        .outline_filter
        .entry
        .emit_by_name::<()>("activate", &[]);
    if !filtered {
        window
            .lookup_action(SidebarPaneKind::Outline.filter_action())
            .expect("the filter command is registered")
            .activate(None);
        until(Clock::Frame, "the filter to clear", || {
            state(window).is_some_and(|st| st.outline_filter.borrow().query().is_none())
        });
    }
}

/// The editor modes: the reader puts the caret in the target section and scrolls it
/// into view — the outline follows the caret here (TDD 12.14).
fn park_in_editor(window: &ApplicationWindow, doc: &str, filtered: bool) {
    if filtered {
        filter_outline(window);
    }
    let st = state(window).expect("tab");
    let it = st
        .editor_buf
        .iter_at_line(target_line(doc) + 2)
        .expect("a line inside the target section");
    st.editor_buf.place_cursor(&it);
    #[expect(
        clippy::disallowed_methods,
        reason = "test fixture positioning the view as a precondition, not a navigation"
    )]
    crate::farscroll::scroll_to_mark_when_ready(
        st.editor.upcast_ref(),
        &st.editor_buf.get_insert(),
        0.0,
        true,
        0.0,
        0.0,
    );
}

fn run(suffix: &str, mode: &str, filtered: bool) {
    let doc = document();
    let window = open(suffix, mode, &doc);
    if mode == "preview" {
        park_in_preview(&window, filtered);
    } else {
        park_in_editor(&window, &doc, filtered);
    }
    let (before_row, before_top, before_caret) = settled(&window);
    assert_eq!(
        before_row.as_deref(),
        Some(TARGET),
        "precondition: the outline reflects the reader's place before the reload"
    );
    assert!(
        before_top > 100,
        "precondition: the reader is far down the document (top line {before_top})"
    );
    assert_eq!(
        state(&window)
            .unwrap()
            .outline_filter
            .borrow()
            .query()
            .is_some(),
        filtered,
        "precondition: the filter is as this case needs it"
    );

    reload_with_appended(&window, &doc);
    let (after_row, after_top, after_caret) = settled(&window);
    assert_reloaded(&window, mode);

    assert_eq!(
        after_row.as_deref(),
        Some(TARGET),
        "{mode} (filtered: {filtered}): the outline's selection comes back to the reader's \
         heading (pane top {before_top} → {after_top})"
    );
    assert_eq!(
        after_top, before_top,
        "{mode} (filtered: {filtered}): the reading pane comes back to the same top line"
    );
    if mode != "preview" {
        assert_eq!(
            after_caret, before_caret,
            "{mode} (filtered: {filtered}): the caret stays where the reader put it"
        );
    }
    window.destroy();
}

/// **3.2 / 12.16, Preview.** Mutation-checked: writing the line's y as the adjustment
/// value (dropping `value_for_top_y`'s margin) brings the top line back one short and
/// the selection back to the previous section — or, filtered, to its shown ancestor.
#[gtktest::test]
fn a_live_reload_keeps_the_outline_on_the_readers_heading_in_preview() {
    run("outline_reload.preview", "preview", false);
}

#[gtktest::test]
fn a_live_reload_keeps_the_filtered_outline_on_the_readers_heading_in_preview() {
    run("outline_reload.preview_filtered", "preview", true);
}

/// **3.2 / 12.16, Edit and Split.** Mutation-checked: without `restore_editor_place` the
/// caret comes back on the first line, the editor at the top, and the selection on the
/// document's first heading.
#[gtktest::test]
fn a_live_reload_keeps_the_caret_and_the_outline_in_edit() {
    run("outline_reload.edit", "edit", false);
}

#[gtktest::test]
fn a_live_reload_keeps_the_caret_and_the_outline_in_split() {
    run("outline_reload.split", "split", false);
}

#[gtktest::test]
fn a_live_reload_keeps_the_caret_and_the_filtered_outline_in_split() {
    run("outline_reload.split_filtered", "split", true);
}

/// **3.2, Preview, a reader who scrolled by hand.** The reload restores the line the
/// view TRACKED while the reader scrolled, so that line must be the one at the top
/// EDGE — the space the restore writes back into. The reader is left a few pixels short
/// of a heading, with the last line of the previous section at the top.
///
/// Mutation-checked: tracking `line_at_y(value)` (a margin below the edge) records the
/// heading instead, and the reload brings the heading to the top — one line on, and the
/// outline a section on.
#[gtktest::test]
fn a_live_reload_returns_a_hand_scrolled_preview_to_the_same_line() {
    use gtk::subclass::prelude::*;
    let doc = document();
    let window = open("outline_reload.preview_hand", "preview", &doc);
    park_in_preview(&window, false);
    settled(&window);
    let view = state(&window)
        .unwrap()
        .split
        .preview_scroller()
        .and_then(|sw| sw.child())
        .and_then(|c| c.downcast::<crate::codeview::CodePreviewView>().ok())
        .expect("a preview view");
    let vadj = view.vadjustment().expect("the view is in a scroller");
    // Scroll the way a reader does — `user_scrolling` is what every hand-scroll input
    // source sets — to just above the heading the navigation parked at the top.
    view.imp().user_scrolling.set(true);
    crate::saferizer::scrollpos::jump(&vadj, vadj.value() - 4.0);
    let (before_row, before_top, _) = settled(&window);
    assert_ne!(
        before_row.as_deref(),
        Some(TARGET),
        "precondition: the line at the top is the previous section's"
    );

    reload_with_appended(&window, &doc);
    let (after_row, after_top, _) = settled(&window);
    assert_reloaded(&window, "preview");
    assert_eq!(
        after_top, before_top,
        "the hand-scrolled reading position comes back to the same top line"
    );
    assert_eq!(after_row, before_row, "and the outline with it");
    window.destroy();
}

/// The split preview's view, and the line in it that holds [`TARGET`].
fn split_preview_and_heading_line(
    window: &ApplicationWindow,
) -> (crate::codeview::CodePreviewView, i32) {
    let view = state(window)
        .expect("tab")
        .split
        .preview_scroller()
        .and_then(|sw| sw.child())
        .and_then(|c| c.downcast::<crate::codeview::CodePreviewView>().ok())
        .expect("a split preview view");
    let heading = view
        .buffer()
        .start_iter()
        .forward_search(TARGET, gtk::TextSearchFlags::empty(), None)
        .map(|(start, _)| start.line())
        .expect("the preview carries the target heading");
    (view, heading)
}

/// The two panes' top lines once both have stopped moving: `(editor, preview)`.
fn settled_split_tops(window: &ApplicationWindow) -> (i32, i32) {
    let st = state(window).expect("tab");
    let (view, _) = split_preview_and_heading_line(window);
    let s = until_stable(
        Clock::Frame,
        Duration::from_secs(15),
        Duration::from_millis(800),
        || {
            (
                crate::saferizer::viewport::ViewportTopIter::of(&st.editor).line(),
                crate::saferizer::viewport::ViewportTopIter::of(&view).line(),
            )
        },
    );
    assert!(s.converged, "the split panes never settled");
    s.value.expect("sampled at least once")
}

/// What the block before [`TARGET`] is. The render's source map differs at the heading
/// line's first byte between the two, so a lookup can be right for one and wrong for the
/// other — MEASURED: a fix that held after a paragraph still put the end of a LIST above
/// the heading on `sdd/TDD.md`.
#[derive(Clone, Copy, Debug)]
enum Before {
    Paragraph,
    List,
}

fn document_with(before: Before) -> String {
    let doc = document();
    match before {
        Before::Paragraph => doc,
        Before::List => doc.replace(
            &format!("### {TARGET}\n"),
            &format!("- a list item\n- and another\n\n### {TARGET}\n"),
        ),
    }
}

/// Park on [`TARGET`] in Split through the outline, which makes the PREVIEW the
/// driver, and check both panes show the heading at the top before anything else.
fn park_split_on_heading(suffix: &str, before: Before) -> (ApplicationWindow, String) {
    let doc = document_with(before);
    let window = open(suffix, "split", &doc);
    park_in_preview(&window, false);
    let (editor_top, preview_top) = settled_split_tops(&window);
    let (_, heading) = split_preview_and_heading_line(&window);
    assert_eq!(
        preview_top, heading,
        "precondition ({before:?}): the outline put the heading at the top of the preview"
    );
    assert_eq!(
        editor_top,
        target_line(&doc),
        "precondition ({before:?}): the editor followed the preview onto the heading's line"
    );
    assert_eq!(
        state(&window).expect("tab").editor.top_margin(),
        0,
        "the editor has no top margin, so the preview-to-editor half of the margin fix \
         is not observable here (the fact `project_scroll`'s comment relies on)"
    );
    (window, doc)
}

/// The preview's top line must be the heading; the message names what is there instead.
fn assert_preview_top_is_the_heading(window: &ApplicationWindow, what: &str) {
    let (editor_top, preview_top) = settled_split_tops(window);
    let (view, heading) = split_preview_and_heading_line(window);
    let buf = view.buffer();
    let start = buf.iter_at_line(preview_top).expect("a line in range");
    let mut end = start;
    if !end.ends_line() {
        end.forward_to_line_end();
    }
    assert_eq!(
        preview_top,
        heading,
        "{what}: the preview's top line must stay the heading the editor is on (editor top \
         line {editor_top}); it is line {preview_top}, {:?}",
        buf.slice(&start, &end, false)
            .chars()
            .take(40)
            .collect::<String>()
    );
}

/// What `rerender_split_preview_driven_by_editor` does, without the re-render: the
/// editor drives, and both panes' de-dup records are cleared so the projection runs.
fn hand_the_driver_to_the_editor(window: &ApplicationWindow) {
    let st = state(window).expect("tab");
    st.scroll.driver.set(ScrollDriver::Editor);
    st.scroll.ed_last.set((-1.0, -1.0));
    st.scroll.pv_last.set((-1.0, -1.0));
    super::scrollsync::queue_scroll_sync(window);
}

/// **TDD 7.5, the line-accurate half, with the EDITOR driving.** A reader parked on a
/// heading in Split who hands the driver to the editor (a scroll of the editor, or any
/// live-preview re-render, which always makes the editor drive) must still see the
/// heading at the top of the preview, not the tail of the previous section.
///
/// Mutation-checked, each half of the fix alone, after a paragraph: writing the line's
/// buffer y as the adjustment value (no `value_for_top_y`) leaves the previous section's
/// last line on top; the at-or-before lookup in place of `line_to_preview_char` targets
/// the paragraph above the heading. Each fails here.
#[gtktest::test]
fn handing_the_split_driver_to_the_editor_keeps_a_heading_at_the_top_of_the_preview() {
    let (window, _doc) = park_split_on_heading("split_heading.editor_drives", Before::Paragraph);
    hand_the_driver_to_the_editor(&window);
    assert_preview_top_is_the_heading(&window, "after a paragraph");
    window.destroy();
}

/// The same, with a LIST before the heading. Mutation-checked: stopping at a waypoint
/// AT the line's first byte (the map's entry for the end of the list) puts the list's
/// last item on top.
#[gtktest::test]
fn handing_the_split_driver_to_the_editor_keeps_a_heading_after_a_list_at_the_top() {
    let (window, _doc) = park_split_on_heading("split_heading.editor_drives_list", Before::List);
    hand_the_driver_to_the_editor(&window);
    assert_preview_top_is_the_heading(&window, "after a list");
    window.destroy();
}

/// **TDD 3.2a in Split, parked ON a heading.** A live reload re-renders the preview
/// with the editor driving, so this is the projection above reached by the route the
/// report came from. The Split reload cases above park in a paragraph, where the
/// defect has nothing to show.
#[gtktest::test]
fn a_live_reload_keeps_a_heading_at_the_top_of_the_split_preview() {
    for (suffix, before) in [
        ("split_heading.reload", Before::Paragraph),
        ("split_heading.reload_list", Before::List),
    ] {
        let (window, doc) = park_split_on_heading(suffix, before);
        reload_with_appended(&window, &doc);
        assert_preview_top_is_the_heading(&window, &format!("after a reload, {before:?}"));
        assert_reloaded(&window, "split");
        window.destroy();
    }
}

/// The preview pane's view and top line once it has stopped moving, plus the line in it
/// that holds [`TARGET`].
fn settled_preview_top_and_heading(window: &ApplicationWindow) -> (i32, i32, String) {
    let (view, heading) = split_preview_and_heading_line(window);
    let s = until_stable(
        Clock::Frame,
        Duration::from_secs(15),
        Duration::from_millis(800),
        || crate::saferizer::viewport::ViewportTopIter::of(&view).line(),
    );
    assert!(s.converged, "the preview never settled");
    let top = s.value.expect("sampled at least once");
    let buf = view.buffer();
    let start = buf.iter_at_line(top).expect("a line in range");
    let mut end = start;
    if !end.ends_line() {
        end.forward_to_line_end();
    }
    let text = buf.slice(&start, &end, false).chars().take(40).collect();
    (top, heading, text)
}

/// **TDD 7.5, a mode switch from Edit.** A reader with a heading on the editor's top line
/// who switches to Preview or Side by Side must see that heading at the top of the
/// preview, not the end of the block above it, whether a paragraph or a list precedes
/// it.
///
/// Mutation-checked: the at-or-before lookup (`to_preview_char`) in place of
/// `reading_to_preview_char` in `apply_content_reading_position` opened Preview two lines
/// up, on the end of the paragraph or list. Side by Side passes either way here, because
/// its editor-driven sync re-projects the preview with the line lookup once the switch
/// lands; it runs so that a regression there is caught too.
#[gtktest::test]
fn a_mode_switch_from_edit_keeps_a_heading_at_the_top_of_the_preview() {
    for before in [Before::Paragraph, Before::List] {
        for to in ["preview", "split"] {
            let doc = document_with(before);
            let window = open(
                &format!("mode_switch_heading.{before:?}.{to}"),
                "edit",
                &doc,
            );
            let st = state(&window).expect("tab");
            let it = st
                .editor_buf
                .iter_at_line(target_line(&doc))
                .expect("the heading line");
            st.editor_buf.place_cursor(&it);
            #[expect(
                clippy::disallowed_methods,
                reason = "test fixture positioning the view as a precondition, not a navigation"
            )]
            crate::farscroll::scroll_to_mark_when_ready(
                st.editor.upcast_ref(),
                &st.editor_buf.get_insert(),
                0.0,
                true,
                0.0,
                0.0,
            );
            let ed = until_stable(
                Clock::Frame,
                Duration::from_secs(15),
                Duration::from_millis(800),
                || crate::saferizer::viewport::ViewportTopIter::of(&st.editor).line(),
            );
            let ed_top = ed.value.expect("sampled");
            window.change_action_state("view-mode", &to.to_variant());
            let (top, heading, text) = settled_preview_top_and_heading(&window);
            assert_eq!(
                ed_top,
                target_line(&doc),
                "precondition: the heading tops the editor"
            );
            assert_eq!(
                top, heading,
                "{before:?}, edit to {to}: the preview must open on the heading; its top \
                 line is {top}, {text:?}"
            );
            window.destroy();
        }
    }
}
