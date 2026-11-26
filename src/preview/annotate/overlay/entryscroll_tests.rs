//! The preview's comment card through a scroll (TDD 17.25): a card the reader is
//! writing in stays open and moves with its text. Before the fix it was placed once,
//! on opening, and sat still while its text scrolled away.
//!
//! In its own file because `overlay.rs` is past the soft line limit.

use crate::codeview::CodePreviewView;
use crate::testpump::{drain_for, until, until_or_for, Clock};
use gtk::prelude::*;
use std::time::Duration;

const SCROLL_BY: f64 = 60.0;

fn document() -> String {
    (0..80)
        .map(|i| format!("Paragraph {i} of a document tall enough to scroll.\n\n"))
        .collect()
}

#[gtktest::test]
fn an_open_comment_card_moves_with_its_text_when_the_preview_scrolls() {
    let win = gtk::Window::new();
    let pane = crate::preview::render(
        None,
        &document(),
        None,
        1.0,
        false,
        &crate::fold::FoldState::default(),
    );
    let overlay = pane
        .clone()
        .downcast::<gtk::Overlay>()
        .expect("preview pane is a GtkOverlay");
    let scroller = overlay
        .child()
        .and_downcast::<gtk::ScrolledWindow>()
        .expect("overlay wraps the scroller");
    let view = scroller
        .child()
        .and_downcast::<CodePreviewView>()
        .expect("scroller holds the CodePreviewView");
    view.set_annotation_sink(std::rc::Rc::new(|_| {}));
    win.set_default_size(700, 500);
    win.set_child(Some(&pane));
    win.present();
    until(Clock::Frame, "the preview to map and overflow", || {
        view.is_mapped() && scroller.vadjustment().upper() > 1000.0
    });
    drain_for(Clock::Frame, Duration::from_millis(300));

    // A word on a paragraph in the first screen, low enough that the card sits above it.
    let buf = view.buffer();
    let (word, word_end) = buf
        .start_iter()
        .forward_search("Paragraph 6 ", gtk::TextSearchFlags::empty(), None)
        .expect("the paragraph rendered");
    buf.select_range(&word, &word_end);
    drain_for(Clock::Frame, Duration::from_millis(200));
    assert!(view.trigger_annotate(), "the annotate trigger is wired");

    let card = || {
        let mut child = overlay.first_child();
        while let Some(c) = child {
            if c.has_css_class(crate::window::ANNOTATION_CARD_CLASS) {
                return Some(c);
            }
            child = c.next_sibling();
        }
        None
    };
    until(Clock::Frame, "the comment card to open", || {
        card().is_some_and(|c| c.is_visible())
    });
    drain_for(Clock::Frame, Duration::from_millis(200));
    let bar = card().expect("the comment card is an overlay child");
    let before = bar.margin_top();

    let vadj = scroller.vadjustment();
    crate::saferizer::scrollpos::jump(&vadj, vadj.value() + SCROLL_BY);
    let _ = until_or_for(Clock::Frame, Duration::from_secs(3), || {
        bar.margin_top() != before
    });
    drain_for(Clock::Frame, Duration::from_millis(200));

    assert!(bar.is_visible(), "scrolling must not close the card");
    let moved = before - bar.margin_top();
    assert!(
        (40..=80).contains(&moved),
        "the card must move up with its text by about the {SCROLL_BY} px scrolled; it moved \
         {moved} px (from {before})"
    );
    win.destroy();
}
