//! A suggested edit's card in a real window: Edit Suggestion opens on what the
//! suggestion adds, as written, and saving rewrites only that side (TDD 17.78).

use super::{find_button, find_descendant_where, hitbox_count, settle};
use crate::annotations::Direction;
use crate::codeview::markerrows::{EDIT_NOTE, EDIT_SUGGESTION};
use crate::codeview::CodePreviewView;
use gtk::prelude::*;

/// Open the only chip's card in a window showing `doc`, returning the window and view.
fn open_card(id: &str, doc: &str) -> (gtk::ApplicationWindow, CodePreviewView) {
    let app = crate::window::testkit::test_app(&format!(
        "com.extollit.scribobulate.integrationtest.{id}"
    ));
    let win = crate::window::new_window(&app, id, doc, None);
    let view = super::find_descendant::<CodePreviewView>(&win).expect("the preview view");
    settle("the chip to paint", || hitbox_count(&view) == 1);
    assert!(view.open_stepped_marker_popover(0, Direction::Next));
    settle("the card to open", || view.has_open_marker_popover());
    (win, view)
}

fn card(view: &CodePreviewView) -> crate::codeview::card::AnnotationCard {
    use gtk::subclass::prelude::*;
    view.imp()
        .marker_card
        .borrow()
        .clone()
        .expect("the card exists once opened")
}

/// The field on the editor page now showing.
fn shown_field(view: &CodePreviewView) -> sourceview::View {
    let mut found = None;
    settle("an editor page to show", || {
        found = find_descendant_where::<sourceview::View>(&card(view), |f| f.is_mapped());
        found.is_some()
    });
    found.unwrap()
}

#[gtktest::test]
fn edit_suggestion_rewrites_what_a_replacement_adds_and_keeps_the_card_open() {
    let (win, view) = open_card("editsuggest", "A {~~cat~>**dog**~~}{>>pet<<} sat.\n");
    assert!(
        find_button(&card(&view), EDIT_NOTE).is_some(),
        "the note stays editable"
    );
    find_button(&card(&view), EDIT_SUGGESTION)
        .expect("a replacement offers Edit Suggestion")
        .emit_clicked();
    let field = shown_field(&view);
    assert_eq!(
        crate::widgets::comment_entry::comment_text(&field),
        "**dog**",
        "the field holds the new side as written, formatting and all"
    );
    crate::widgets::comment_entry::set_comment_text(&field, "*fox*");
    crate::widgets::comment_entry::press_key(&field, gtk::gdk::Key::Return);
    let source = || crate::window::document_source(&win).unwrap_or_default();
    settle("the new side to reach the document", || {
        source() == "A {~~cat~>*fox*~~}{>>pet<<} sat.\n"
    });
    settle("the card to come back on the edited suggestion", || {
        view.has_open_marker_popover() && find_button(&card(&view), EDIT_SUGGESTION).is_some()
    });
    crate::testpump::destroy_after_tooltip_delay(win.upcast_ref(), || {
        view.popdown_marker_popover()
    });
}

#[gtktest::test]
fn only_an_addition_or_a_replacement_offers_edit_suggestion() {
    for (id, doc, offered) in [
        ("editsuggest-ins", "A {++dog++} sat.\n", true),
        ("editsuggest-del", "A {--dog--} sat.\n", false),
        ("editsuggest-hl", "A {==dog==} sat.\n", false),
    ] {
        let (win, view) = open_card(id, doc);
        assert_eq!(
            find_button(&card(&view), EDIT_SUGGESTION).is_some(),
            offered,
            "{doc:?}"
        );
        assert!(find_button(&card(&view), EDIT_NOTE).is_some(), "{doc:?}");
        crate::testpump::destroy_after_tooltip_delay(win.upcast_ref(), || {
            view.popdown_marker_popover()
        });
    }
}
