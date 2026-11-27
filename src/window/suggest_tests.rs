//! TDD 17.79: `win.suggest` writes a suggested addition, replacement or deletion over the
//! selection, from either pane, through a real window.

use super::*;
use crate::testpump::{drain_for, until, Clock};
use std::time::Duration;

/// Long enough for a deferred write (an idle) to run.
const SETTLE: Duration = Duration::from_millis(300);
const DOC: &str = "The cat sat.\n\nA dog ran.\n";

fn document_window(id: &str) -> (ApplicationWindow, crate::codeview::CodePreviewView) {
    let app = crate::window::testkit::test_app(&format!(
        "com.extollit.scribobulate.integrationtest.{id}"
    ));
    let win = crate::window::new_window(&app, id, DOC, None);
    win.present();
    let view = crate::window::zoom::get_preview_view(&win).expect("a preview");
    until(Clock::Frame, "the preview to lay out", || view.height() > 0);
    (win, view)
}

/// Select `needle` in the preview's text.
fn select_in_preview(view: &crate::codeview::CodePreviewView, needle: &str) {
    let buf = view.buffer();
    let text = crate::saferizer::BufferText::of(&buf).into_string();
    let at = text[..text.find(needle).unwrap()].chars().count() as i32;
    let len = needle.chars().count() as i32;
    buf.select_range(&buf.iter_at_offset(at), &buf.iter_at_offset(at + len));
}

/// Activate `win.suggest::<kind>`, which the selection must have enabled.
fn suggest(win: &ApplicationWindow, kind: crate::annotate::suggest::SuggestKind) {
    crate::window::update_annotate_action_state(win);
    let action = win.lookup_action("suggest").expect("win.suggest");
    assert!(
        action.is_enabled(),
        "precondition: a selection enables win.suggest"
    );
    assert!(
        win.lookup_action(crate::window::ANNOTATE_AVAILABLE)
            .is_some_and(|a| a.is_enabled()),
        "the submenu row is live with its items"
    );
    action.activate(Some(&kind.target().to_variant()));
}

/// The card's field, once it has the focus, holding `expected`; then `typed` + Enter.
fn type_into_card(win: &ApplicationWindow, expected: &str, typed: &str) {
    until(Clock::Frame, "the card to take the focus", || {
        GtkWindowExt::focus(win)
            .is_some_and(|f| f.has_css_class(crate::widgets::comment_entry::COMMENT_FIELD_CLASS))
    });
    let field = GtkWindowExt::focus(win)
        .and_downcast::<sourceview::View>()
        .expect("the card's field holds the focus");
    assert_eq!(
        crate::widgets::comment_entry::comment_text(&field),
        expected
    );
    crate::widgets::comment_entry::set_comment_text(&field, typed);
    assert!(crate::widgets::comment_entry::press_key(
        &field,
        gtk::gdk::Key::Return
    ));
    drain_for(Clock::Frame, SETTLE);
}

fn source(win: &ApplicationWindow) -> String {
    crate::window::document_source(win).unwrap_or_default()
}

#[gtktest::test]
fn each_suggestion_is_written_from_the_preview() {
    use crate::annotate::suggest::SuggestKind;
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let (win, view) = document_window("suggestpreview");
        select_in_preview(&view, "cat");
        suggest(&win, SuggestKind::Delete);
        until(Clock::Frame, "the deletion", || {
            source(&win).contains("{--cat--}")
        });

        select_in_preview(&view, "dog");
        suggest(&win, SuggestKind::Replace);
        type_into_card(&win, "dog", "fox");
        until(Clock::Frame, "the replacement", || {
            source(&win).contains("A {~~dog~>fox~~} ran.")
        });

        select_in_preview(&view, "ran");
        suggest(&win, SuggestKind::Add);
        type_into_card(&win, "", " away");
        until(Clock::Frame, "the addition", || {
            source(&win).contains("ran{++ away++}.")
        });

        // Across two paragraphs: refused, nothing written.
        let before = source(&win);
        let buf = view.buffer();
        buf.select_range(&buf.start_iter(), &buf.end_iter());
        suggest(&win, SuggestKind::Delete);
        drain_for(Clock::Frame, SETTLE);
        assert_eq!(source(&win), before);
        crate::testpump::destroy_after_tooltip_delay(win.upcast_ref(), || {});
    });
}

#[gtktest::test]
fn a_suggestion_is_written_from_the_editor() {
    use crate::annotate::suggest::SuggestKind;
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let (win, _view) = document_window("suggesteditor");
        crate::window::change_action_state(&win, "view-mode", &"edit".to_variant());
        let st = state(&win).expect("state");
        let editor = st.editor.clone();
        until(Clock::Frame, "the editor to lay out", || {
            editor.height() > 0
        });
        editor.grab_focus();
        let buf = st.editor_buf.clone();
        let at = DOC.find("cat").unwrap() as i32;
        buf.select_range(&buf.iter_at_offset(at), &buf.iter_at_offset(at + 3));
        suggest(&win, SuggestKind::Replace);
        type_into_card(&win, "cat", "dog");
        until(Clock::Frame, "the replacement", || {
            source(&win).starts_with("The {~~cat~>dog~~} sat.")
        });
        crate::testpump::destroy_after_tooltip_delay(win.upcast_ref(), || {});
    });
}

/// The toolbar's Annotate section (TDD 17.80): hidden by default, its four buttons are the
/// Edit ▸ Annotate items, grey without a selection, and a click writes the same thing the
/// menu item does.
#[gtktest::test]
fn the_toolbar_annotate_section_drives_the_same_commands() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let (win, view) = document_window("suggesttoolbar");
        let at = crate::app::TBTN_SECTION_IDS
            .iter()
            .position(|id| *id == "annotate")
            .expect("an annotate section");
        let items = crate::winstate::chrome(&win)
            .expect("chrome")
            .toolbar_sections[at]
            .clone();
        assert!(
            items.iter().all(|w| !w.is_visible()),
            "the Annotate section is hidden on a fresh profile"
        );
        let buttons: Vec<gtk::Button> = items
            .iter()
            .filter_map(|w| w.clone().downcast::<gtk::Button>().ok())
            .collect();
        let bound: Vec<_> = buttons
            .iter()
            .map(|b| {
                (
                    b.label().map(|l| l.to_string()),
                    b.action_name().map(|a| a.to_string()),
                    b.action_target_value().and_then(|t| t.get::<String>()),
                )
            })
            .collect();
        let expected: Vec<_> = crate::app::ANNOTATE_SUBMENU
            .iter()
            .map(|(l, a, t)| {
                (
                    Some(l.to_string()),
                    Some(a.to_string()),
                    t.map(str::to_string),
                )
            })
            .collect();
        assert_eq!(
            bound, expected,
            "the section's buttons are the submenu's items"
        );
        let format_at = crate::app::TBTN_SECTION_IDS
            .iter()
            .position(|id| *id == "format")
            .expect("a format section");
        assert!(
            crate::winstate::chrome(&win)
                .expect("chrome")
                .toolbar_sections[format_at]
                .iter()
                .filter_map(|w| w.dynamic_cast_ref::<gtk::Actionable>())
                .all(|a| a.action_name().as_deref() != Some("win.annotate")),
            "Comment is in the Annotate section, not the Format section"
        );

        let app = win.application().expect("an application");
        app.change_action_state(
            &toolbarchrome::section_action("annotate"),
            &true.to_variant(),
        );
        drain_for(Clock::Frame, SETTLE);
        assert!(
            items.iter().all(|w| w.is_visible()),
            "View ▸ Toolbar ▸ Annotate shows it"
        );

        crate::window::update_annotate_action_state(&win);
        assert!(
            buttons.iter().all(|b| !b.is_sensitive()),
            "with nothing selected every Annotate button is grey"
        );

        select_in_preview(&view, "cat");
        crate::window::update_annotate_action_state(&win);
        let delete = buttons
            .iter()
            .find(|b| b.label().as_deref() == Some("Delete"))
            .expect("a Delete button");
        assert!(delete.is_sensitive(), "a selection enables the section");
        delete.emit_clicked();
        until(Clock::Frame, "the deletion", || {
            source(&win).contains("{--cat--}")
        });
        crate::testpump::destroy_after_tooltip_delay(win.upcast_ref(), || {});
    });
}
