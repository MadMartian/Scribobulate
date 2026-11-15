//! The Annotate card through a window resize that moves its word up (TDD 17.64).
//!
//! In its own file because `overlay.rs` is past the soft line limit.

use super::action_card_of;
use crate::codeview::CodePreviewView;
use crate::testpump::{destroy_after_tooltip_delay, drain_for, until, until_or_for, Clock};
use crate::widgets::floating_card::FloatingCardExt;
use gtk::prelude::*;
use std::time::Duration;

/// Narrow, so the lead paragraphs and the target paragraph wrap.
const NARROW: i32 = 320;
/// Wide, so none of them does.
const WIDE: i32 = 1000;
const HEIGHT: i32 = 400;
/// Room the word needs above it for the card to sit there: the card is 41 px high on macOS,
/// plus its gap to the word and a margin for a taller one elsewhere.
const CARD_ROOM: i32 = 80;

/// Lead paragraphs that wrap when narrow, so widening shortens what is above the viewport
/// and the view moves its scroll value to keep the target at the top; then the target,
/// long enough to wrap onto a third line when narrow and fit one line when wide.
fn document() -> String {
    let mut md = String::new();
    for i in 0..40 {
        md.push_str(&format!(
            "Lead paragraph {i} with enough words in it to wrap onto a second line when \
             narrow.\n\n"
        ));
    }
    md.push_str(
        "Target alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi \
         omicron pi rho sigma tau upsilon phi chi psi omega alpha beta gamma delta epsilon \
         zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi \
         psi omega alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu.\n\n",
    );
    for i in 0..40 {
        md.push_str(&format!("Trail paragraph {i}.\n\n"));
    }
    md
}

/// `iter`'s line box in `overlay`'s coordinates: `(x, top, bottom)`.
fn line_box_in(
    view: &CodePreviewView,
    overlay: &gtk::Overlay,
    iter: &gtk::TextIter,
) -> (f64, f64, f64) {
    let loc = view.iter_location(iter);
    let (wx, wy) = view.buffer_to_window_coords(gtk::TextWindowType::Widget, loc.x(), loc.y());
    let (x, y) = view
        .translate_coordinates(overlay, f64::from(wx), f64::from(wy))
        .expect("the view is inside its pane");
    (x, y, y + f64::from(loc.height()))
}

/// **A resize that moves the selected word up to the top line flips the Annotate card
/// below it, rather than closing it or leaving it pointing where the word was.**
///
/// The narrow window has the card above a word on the target paragraph's third line.
/// Widening it re-wraps everything: the lead paragraphs above the viewport shrink, so the
/// view moves its scroll value to keep the target paragraph at the top, and the word moves
/// up to the first line, where there is no room above it. The scroll-hide read that value
/// change as a scroll and closed the card, and nothing re-pinned the card, so
/// even a card left up kept pointing at the old third line.
///
/// Mutation: comparing the scroll value rather than `PaintedTop` in the scroll-hide closes
/// the card here; dropping the width hook leaves it pointing where the word was.
#[gtktest::test]
fn a_resize_that_moves_the_word_up_flips_the_annotate_card_below_it() {
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
    // The card only shows with an annotation sink (the split view wires one).
    view.set_annotation_sink(std::rc::Rc::new(|_| {}));
    win.set_default_size(NARROW, HEIGHT);
    win.set_child(Some(&pane));
    win.present();
    until(Clock::Frame, "the preview to map", || {
        view.is_mapped() && view.height() > 0
    });
    drain_for(Clock::Frame, Duration::from_millis(300));

    // The target paragraph at the top of the viewport.
    let buf = view.buffer();
    let (target, _) = buf
        .start_iter()
        .forward_search("Target alpha", gtk::TextSearchFlags::empty(), None)
        .expect("the target paragraph rendered");
    let target = target.offset();
    let vadj = scroller.vadjustment();
    crate::saferizer::scrollpos::jump(
        &vadj,
        f64::from(view.iter_location(&buf.iter_at_offset(target)).y()),
    );
    drain_for(Clock::Frame, Duration::from_millis(300));

    // A word far enough down the narrow paragraph that the card fits above it. A fixed third
    // line did not: where a line is ~22 px the third starts 44 px down, under the card's 41 px
    // plus its gap, so the card was already below the word before the resize (macOS) and the
    // test measured nothing. The line is found from the card's own room, not counted.
    let room_above = f64::from(CARD_ROOM);
    let mut word = buf.iter_at_offset(target);
    let top_of = |it: &gtk::TextIter| line_box_in(&view, &overlay, it).1;
    let top_at_start = top_of(&word);
    while top_of(&word) - top_at_start < room_above {
        if !view.forward_display_line(&mut word) {
            break;
        }
    }
    let mut word_end = word;
    word_end.forward_word_end();
    buf.select_range(&word, &word_end);
    let card = action_card_of(&view).expect("the action card is wired onto the view");
    until(
        Clock::Frame,
        "the Annotate card to rise over the word",
        || card.is_visible(),
    );
    drain_for(Clock::Frame, Duration::from_millis(300));
    let (_, word_top_before, _) = line_box_in(&view, &overlay, &word);
    let bounds = card.compute_bounds(&overlay).expect("the card is laid out");
    assert!(
        f64::from(bounds.y() + bounds.height()) <= word_top_before,
        "precondition: the card sits above the word on the third line: card {bounds:?}, word \
         top {word_top_before}"
    );
    let value_before = vadj.value();

    crate::window::testkit::resize_mapped_window(&win, WIDE, HEIGHT);
    until(Clock::Frame, "the window to widen", || win.width() >= WIDE);
    // The re-wrap above the viewport is validated lazily and can land frames after the window
    // reports its new width: on macOS the word had already moved up with its own paragraph's
    // re-wrap while the scroll value still read its old figure, so the precondition below
    // measured a half-finished reflow. Wait for the value to move (bounded; the precondition
    // still asserts it) before reading anything else.
    let _moved = until_or_for(Clock::Frame, Duration::from_secs(5), || {
        vadj.value() != value_before
    });
    // The card's re-pin is debounced (~40 ms) and then waits for a paint.
    let flipped = until_or_for(Clock::Frame, Duration::from_secs(2), || {
        let (_, _, word_bottom) = line_box_in(&view, &overlay, &word);
        card.is_visible()
            && card
                .compute_bounds(&overlay)
                .is_some_and(|b| f64::from(b.y()) >= word_bottom)
    });
    let (word_x, word_top, word_bottom) = line_box_in(&view, &overlay, &word);
    assert!(
        // No room above is the placement's own rule: the card's height plus its gap.
        word_top < word_top_before
            && word_top < f64::from(bounds.height() + crate::widgets::floating_card::GAP as f32),
        "precondition: the resize must move the word up to where the card has no room above \
         it, or nothing is tested: word top {word_top_before} -> {word_top}, card {bounds:?}"
    );
    assert_ne!(
        vadj.value(),
        value_before,
        "precondition: the resize must move the scroll value, as a re-wrap above the \
         viewport does, or the scroll-hide is not exercised"
    );
    assert!(
        card.is_visible(),
        "the resize closed the Annotate card: the scroll-hide took the reflow's scroll-value \
         change for a scroll"
    );
    assert!(
        flipped,
        "the Annotate card did not flip below the word the resize moved to the top line: \
         card {:?}, word at ({word_x}, {word_top}..{word_bottom}), pointing at {:?}",
        card.compute_bounds(&overlay),
        card.pointing_to()
    );
    destroy_after_tooltip_delay(&win, || card.popdown());
}
