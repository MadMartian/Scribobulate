//! The action popover's scroll-hide against a fractional wheel-scroll value.
//!
//! In its own file because `overlay.rs` is past the soft line limit.

use crate::codeview::CodePreviewView;
use crate::testpump::{destroy_after_tooltip_delay, drain_for, until, until_or_for, Clock};
use crate::widgets::floating_card::FloatingCardExt;
use gtk::prelude::*;
use std::time::Duration;

/// The action card (`.annotation-overlay`) wired onto `view`.
fn action_popover(view: &CodePreviewView) -> crate::widgets::floating_card::FloatingCard {
    use gtk::subclass::prelude::*;
    view.imp()
        .overlay_popover
        .borrow()
        .clone()
        .expect("the action card is wired onto the preview view")
}

/// A wheel scroll leaves the preview's vadjustment at a fraction, and the view's next
/// allocation writes the truncated value back: a `value-changed` that moves no text. The
/// scroll-hide must not take it for a scroll, or the first selection after a wheel scroll
/// shows no Annotate card.
///
/// The fraction is planted with a plain adjustment write (`#[gtktest::test]` cannot
/// synthesise a wheel event), and asserted still present just before the selection, so
/// the test cannot pass on a value an earlier frame already snapped. The allocation is
/// planted too, once the card is open. On GTK 4.6, where the write-back was measured, the
/// test asserts it happened; on any other GTK it reports a skip for that half instead.
/// Mutation check: hiding on every `value-changed` fails here on GTK 4.6.
#[gtktest::test]
fn the_first_selection_after_a_fractional_scroll_keeps_the_annotate_popover() {
    let mut md = String::new();
    for i in 0..300 {
        md.push_str(&format!(
            "Paragraph {i} carries some selectable prose here.\n\n"
        ));
    }
    let win = gtk::Window::new();
    let pane = crate::preview::render(
        None,
        &md,
        None,
        1.0,
        false,
        &crate::fold::FoldState::default(),
    );
    let scroller = pane
        .downcast_ref::<gtk::Overlay>()
        .expect("preview pane is a GtkOverlay")
        .child()
        .and_downcast::<gtk::ScrolledWindow>()
        .expect("overlay wraps the scroller");
    let view = scroller
        .child()
        .and_downcast::<CodePreviewView>()
        .expect("scroller holds the CodePreviewView");
    // The popover only shows with an annotation sink (the split view wires one).
    view.set_annotation_sink(std::rc::Rc::new(|_| {}));
    win.set_default_size(700, 500);
    win.set_child(Some(&pane));
    win.present();

    let vadj = scroller.vadjustment();
    until(
        Clock::Frame,
        "the preview to map with a scroll range",
        || view.is_mapped() && vadj.page_size() > 0.0 && vadj.upper() > vadj.page_size() + 600.0,
    );
    drain_for(Clock::Frame, Duration::from_millis(300));

    // Where five wheel steps left the editor in the live reproduction.
    crate::saferizer::scrollpos::jump(&vadj, 416.401_609_200_426_4);
    drain_for(Clock::Frame, Duration::from_millis(300));
    assert!(
        vadj.value().fract() != 0.0,
        "precondition: the preview must still hold the fractional value when the text is \
         selected, or nothing below is tested (it reads {})",
        vadj.value()
    );

    // A few words a third of the way down the viewport: on screen, clear of the edges.
    let vy = view.visible_rect().y() + WidgetExt::height(&view) / 3;
    let (iter, _) = view.line_at_y(vy);
    let buf = view.buffer();
    let a = buf.iter_at_offset(iter.offset());
    let b = buf.iter_at_offset((iter.offset() + 9).min(buf.end_iter().offset()));
    buf.select_range(&a, &b);

    let pop = action_popover(&view);
    // Not a fatal wait: with the defect the popover opens and closes within one frame, so
    // it may never be seen open, and the assertion below names that failure.
    until_or_for(Clock::Frame, Duration::from_secs(2), || pop.is_visible());
    // Plant the write-back: a fresh allocation of the view. Opening the card used to be
    // that allocation, when it was a popover parented to the view; the floating card does
    // not re-allocate the view, so the test asks for the allocation itself.
    view.queue_resize();
    drain_for(Clock::Frame, Duration::from_millis(400));
    assert!(
        pop.is_visible(),
        "the Annotate popover closed on the first selection after a fractional scroll"
    );
    if vadj.value().fract() != 0.0 {
        assert!(
            !crate::saferizer::viewport::truncated_writeback_measured(),
            "precondition: GTK 4.6 writes the truncated scroll value back on the view's \
             allocation, and it still reads {}, so the guard was not exercised",
            vadj.value()
        );
        crate::testsymlink::skipped(
            "TDD 17.5 truncated write-back",
            "this GTK left the fractional scroll value in place when the view was \
             re-allocated, so the write-back the fix guards against did not happen",
        );
    }

    // And a scroll that does move the text still hides it, so the test cannot pass on a
    // scroll-hide that was simply switched off.
    crate::saferizer::scrollpos::jump(&vadj, vadj.value() + 120.0);
    assert!(
        !pop.is_visible(),
        "a scroll that moves the text must still hide the Annotate popover"
    );

    destroy_after_tooltip_delay(&win, || pop.popdown());
}
