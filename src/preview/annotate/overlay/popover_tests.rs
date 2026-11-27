//! The selection popover's shape (TDD 17.79): titled "Annotate", offering Comment and
//! the three suggested edits, each of which raises its own card or writes at once.

use super::{action_card_of, POPOVER_ACTIONS, POPOVER_TITLE};
use crate::codeview::{AnnotationEdit, CodePreviewView, CreateAnnotation};
use crate::testpump::{destroy_after_tooltip_delay, until, Clock};
use crate::widgets::floating_card::FloatingCardExt;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Every descendant of `root` of type `T`, depth first.
fn all<T: IsA<gtk::Widget>>(root: &gtk::Widget) -> Vec<T> {
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(w) = stack.pop() {
        if let Ok(t) = w.clone().downcast::<T>() {
            out.push(t);
        }
        stack.extend(std::iter::successors(w.first_child(), |c| c.next_sibling()));
    }
    out
}

#[gtktest::test]
fn the_popover_is_titled_annotate_and_offers_comment_add_replace_and_delete() {
    let win = gtk::Window::new();
    let pane = crate::preview::render(
        None,
        "The cat sat.\n",
        None,
        1.0,
        false,
        &crate::fold::FoldState::default(),
    );
    let view = pane
        .downcast_ref::<gtk::Overlay>()
        .and_then(|o| o.child())
        .and_downcast::<gtk::ScrolledWindow>()
        .and_then(|s| s.child())
        .and_downcast::<CodePreviewView>()
        .expect("the preview view");
    let sent: Rc<RefCell<Vec<AnnotationEdit>>> = Rc::default();
    view.set_annotation_sink(Rc::new({
        let sent = sent.clone();
        move |e| sent.borrow_mut().push(e)
    }));
    win.set_default_size(700, 400);
    win.set_child(Some(&pane));
    win.present();
    until(Clock::Frame, "the preview to map", || view.height() > 0);
    let card = action_card_of(&view).expect("the action card");
    let buf = view.buffer();
    let text = crate::saferizer::BufferText::of(&buf).into_string();
    let at = text[..text.find("cat").unwrap()].chars().count() as i32;
    buf.select_range(&buf.iter_at_offset(at), &buf.iter_at_offset(at + 3));
    until(Clock::Frame, "the popover to rise", || card.is_visible());

    let root: gtk::Widget = card.clone().upcast();
    let labels: Vec<String> = all::<gtk::Label>(&root)
        .iter()
        .map(|l| l.text().to_string())
        .collect();
    assert!(
        labels.iter().any(|l| l == POPOVER_TITLE),
        "titled: {labels:?}"
    );
    let buttons: Vec<String> = all::<gtk::Button>(&root)
        .iter()
        .filter_map(|b| b.label().map(|l| l.to_string()))
        .collect();
    for (label, _) in POPOVER_ACTIONS {
        assert!(
            buttons.iter().any(|b| b == label),
            "{label:?} in {buttons:?}"
        );
    }
    assert!(buttons.iter().any(|b| b.ends_with("Comment")));
    assert!(
        !buttons.iter().any(|b| b.contains("Annotate")),
        "renamed: {buttons:?}"
    );

    // Delete writes at once, with no card.
    let delete = all::<gtk::Button>(&root)
        .into_iter()
        .find(|b| b.label().as_deref() == Some("Delete"))
        .unwrap();
    delete.emit_clicked();
    until(Clock::Frame, "the deletion to reach the sink", || {
        !sent.borrow().is_empty()
    });
    assert!(matches!(
        sent.borrow()[0],
        AnnotationEdit::Create(CreateAnnotation::Suggest {
            kind: crate::annotate::suggest::SuggestKind::Delete,
            ..
        })
    ));
    destroy_after_tooltip_delay(&win, move || card.popdown());
}
