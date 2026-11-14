//! TDD §17.39: where the keyboard focus goes when an annotation card is committed with Enter.
//!
//! Driven through the real window — `win.annotate` raises the card, the field's own
//! Enter controller commits it — because the defect lives in what the WINDOW does with a
//! focus widget that was just hidden, and only a real window has the widgets GTK hands
//! the focus to instead.

use super::*;
use crate::testpump::{drain_for, until, Clock};
use std::time::Duration;

/// Long enough for the card's hide to reach GTK's after-paint focus move and for the
/// commit's deferred buffer write (an idle) to run.
const SETTLE: Duration = Duration::from_millis(300);

/// A presented window over `md` whose preview has laid out, under its own app id.
fn document_window(id: &str, md: &str) -> (ApplicationWindow, CodePreviewView) {
    let app = crate::window::testkit::test_app(&format!(
        "com.extollit.scribobulate.integrationtest.{id}"
    ));
    let win = crate::window::new_window(&app, "IT-cardfocus", md, None);
    win.present();
    let view = crate::window::zoom::get_preview_view(&win).expect("a preview");
    until(Clock::Frame, "the preview to lay out", || view.height() > 0);
    (win, view)
}

/// The widget holding the keyboard focus, described for a failure message.
fn focus_name(win: &ApplicationWindow) -> String {
    GtkWindowExt::focus(win)
        .map(|w| format!("{} {:?}", w.type_().name(), w.css_classes()))
        .unwrap_or_else(|| "nothing".into())
}

/// Whether the focus is `pane` or inside it.
fn focus_is_in(win: &ApplicationWindow, pane: &impl IsA<gtk::Widget>) -> bool {
    GtkWindowExt::focus(win).is_some_and(|f| &f == pane.as_ref() || f.is_ancestor(pane))
}

/// Raise the card through `win.annotate` over the selection already made, type `comment`
/// into it, and press Enter on its field through the field's own controller.
fn annotate_and_commit_with_enter(win: &ApplicationWindow, comment: &str) {
    crate::window::update_annotate_action_state(win);
    let action = win.lookup_action("annotate").expect("win.annotate");
    assert!(
        action.is_enabled(),
        "precondition: a selection enables win.annotate"
    );
    action.activate(None);
    until(Clock::Frame, "the comment card to take the focus", || {
        GtkWindowExt::focus(win)
            .is_some_and(|f| f.has_css_class(crate::widgets::comment_entry::COMMENT_FIELD_CLASS))
    });
    let field = GtkWindowExt::focus(win)
        .and_downcast::<sourceview::View>()
        .expect("the card's comment field holds the focus");
    crate::widgets::comment_entry::set_comment_text(&field, comment);
    assert!(
        crate::widgets::comment_entry::press_key(&field, gtk::gdk::Key::Return),
        "Enter in the field must be claimed as a commit"
    );
    drain_for(Clock::Frame, SETTLE);
}

/// **TDD 17.39: Enter on the editor's card hands the focus back to the editor**, as
/// Escape and the card's × do. Committing hid the card holding the focus and left GTK to
/// pick a successor after paint: the card's first focusable ancestor, else the window's
/// first focusable widget. MEASURED before the fix: a `GtkPaned` here (Linux, GTK 4.6.9),
/// the tab strip's × on Windows, where the reader's next Enter closed the tab. This test
/// and its preview twin both failed that way before the fix (the mutation check).
#[gtktest::test]
fn committing_the_editor_card_with_enter_returns_the_focus_to_the_editor() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let (win, _preview) = document_window("cardfocuseditor", "# Heading\n\nbody text\n");
        crate::window::change_action_state(&win, "view-mode", &"edit".to_variant());
        let st = state(&win).expect("state");
        let editor = st.editor.clone();
        until(Clock::Frame, "the editor to lay out", || {
            editor.height() > 0
        });
        editor.grab_focus();
        until(Clock::Frame, "the editor to take focus", || {
            editor.has_focus()
        });
        let buf = st.editor_buf.clone();
        buf.select_range(&buf.iter_at_offset(11), &buf.iter_at_offset(15));

        annotate_and_commit_with_enter(&win, "a note");

        assert!(
            crate::saferizer::BufferText::of(&buf)
                .into_string()
                .contains("{>>a note<<}"),
            "precondition: Enter committed the annotation"
        );
        assert!(
            focus_is_in(&win, &editor),
            "after Enter commits the editor's card the focus must be back in the editor; \
             it is on {}",
            focus_name(&win)
        );
        win.destroy();
    });
}

/// TDD 17.39, the preview's card, the same contract: Enter hands the focus back to the preview.
#[gtktest::test]
fn committing_the_preview_card_with_enter_returns_the_focus_to_the_preview() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let (win, view) = document_window("cardfocuspreview", "# Heading\n\nbody text\n");
        view.grab_focus();
        until(Clock::Frame, "the preview to take focus", || {
            view.has_focus()
        });
        let buf = view.buffer();
        buf.select_range(&buf.iter_at_offset(8), &buf.iter_at_offset(12));

        annotate_and_commit_with_enter(&win, "a note");

        let st = state(&win).expect("state");
        until(Clock::Frame, "the annotation to reach the source", || {
            crate::saferizer::BufferText::of(&st.editor_buf)
                .into_string()
                .contains("{>>a note<<}")
        });
        assert!(
            focus_is_in(&win, &view),
            "after Enter commits the preview's card the focus must be back in the preview; \
             it is on {}",
            focus_name(&win)
        );
        win.destroy();
    });
}
