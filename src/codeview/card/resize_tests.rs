//! The comment card through a window resize that moves its chip (TDD 17.64).
//!
//! In its own file because `card.rs` is near the soft line limit.

use super::AnnotationCard;
use crate::annotations::Direction;
use crate::codeview::CodePreviewView;
use crate::testpump::Clock;
use crate::testpump::{destroy_after_tooltip_delay, drain_for, until, until_or_for, until_stable};
use crate::widgets::floating_card::FloatingCardExt;
use gtk::gdk;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use std::time::Duration;

/// Narrow enough that every lead paragraph wraps onto a second line, and wide enough that
/// the card still fits on its usual side, left of the chip.
const NARROW: i32 = 640;
/// Wide enough that no lead paragraph wraps.
/// At most 1000: the CI runners' screens are 1024 wide, and a window asked to be wider never
/// takes the width.
const WIDE: i32 = 1000;
const HEIGHT: i32 = 420;

/// Where the viewport sits when the card opens.
#[derive(Clone, Copy, Debug)]
enum Scroll {
    /// At the top of the document: nothing above the viewport, so a reflow leaves the scroll
    /// value at 0 and no `value-changed` fires.
    AtTop,
    /// Below wrapped lead paragraphs: a reflow re-wraps them, and the view moves its scroll
    /// value to keep the top line where it was.
    BelowWrappedText,
}

impl Scroll {
    /// Lead paragraphs before the annotated one, and which of them is put at the top.
    fn leads(self) -> (usize, usize) {
        match self {
            Self::AtTop => (3, 0),
            Self::BelowWrappedText => (40, 37),
        }
    }
}

/// `leads` paragraphs that wrap when narrow and fit one line when wide, then the annotated
/// paragraph, then a tail so the view can scroll the annotated one off the top.
fn document(leads: usize) -> String {
    let mut md = String::new();
    for i in 0..leads {
        md.push_str(&format!(
            "Lead paragraph {i} carries enough words in it to wrap onto a second line in \
             the narrow window, but not in the wide one.\n\n"
        ));
    }
    md.push_str("The {==annotated claim==}{>>a note<<} sits here.\n\n");
    for i in 0..40 {
        md.push_str(&format!("Trail paragraph {i}.\n\n"));
    }
    md
}

/// The chip's rectangle now, clamped into the viewport the way the card's own re-anchor
/// clamps it; `None` when it is off the viewport.
fn chip_on_viewport(view: &CodePreviewView) -> Option<gdk::Rectangle> {
    let chip = view.marker_chip_rect(0)?;
    crate::saferizer::on_viewport(crate::saferizer::Viewport::of(view), &chip)
}

fn tuple(r: &gdk::Rectangle) -> (i32, i32, i32, i32) {
    (r.x(), r.y(), r.width(), r.height())
}

/// What one resize did, for the assertion messages and the report.
#[derive(Debug)]
struct Reading {
    value: f64,
    chip: Option<(i32, i32, i32, i32)>,
    pointing: Option<(i32, i32, i32, i32)>,
    card: Option<(f32, f32, f32, f32)>,
}

fn read(view: &CodePreviewView, card: &AnnotationCard, pane: &gtk::Widget) -> Reading {
    Reading {
        value: view.vadjustment().map_or(f64::NAN, |a| a.value()),
        chip: chip_on_viewport(view).as_ref().map(tuple),
        pointing: card.pointing_to().as_ref().map(tuple),
        card: card
            .compute_bounds(pane)
            .map(|b| (b.x(), b.y(), b.width(), b.height())),
    }
}

/// Open the card on the chip in a `from`-wide window scrolled per `scroll`, resize it to
/// `to`, and check the card is still up, pointing at where the chip now is, and beside it.
/// Returns the failure, if any; a broken precondition panics.
fn resize_and_check(scroll: Scroll, from: i32, to: i32) -> Option<String> {
    let (leads, top_lead) = scroll.leads();
    let pane = crate::preview::render(
        None,
        &document(leads),
        None,
        1.0,
        false,
        &crate::fold::FoldState::default(),
    );
    let view = pane
        .clone()
        .downcast::<gtk::Overlay>()
        .expect("preview pane is a GtkOverlay")
        .child()
        .and_downcast::<gtk::ScrolledWindow>()
        .expect("overlay wraps the scroller")
        .child()
        .and_downcast::<CodePreviewView>()
        .expect("scroller holds the CodePreviewView");
    let win = gtk::Window::new();
    win.set_default_size(from, HEIGHT);
    win.set_child(Some(&pane));
    win.present();
    until(Clock::Frame, "the preview to map", || {
        view.is_mapped() && view.height() > 0
    });
    crate::window::testkit::resize_mapped_window(&win, from, HEIGHT);
    until(
        Clock::Frame,
        "the window to take its starting width",
        || win.width() == from,
    );
    drain_for(Clock::Frame, Duration::from_millis(300));

    let vadj = view.vadjustment().expect("the view is in a scroller");
    if top_lead > 0 {
        let buf = view.buffer();
        let (lead, _) = buf
            .start_iter()
            .forward_search(
                &format!("Lead paragraph {top_lead} "),
                gtk::TextSearchFlags::empty(),
                None,
            )
            .expect("the lead paragraph rendered");
        let y = view.iter_location(&lead).y();
        crate::saferizer::scrollpos::jump(&vadj, f64::from(y));
    }
    until(Clock::Frame, "the chip to paint", || {
        !view.imp().marker_hitboxes.borrow().is_empty()
    });
    assert!(
        view.open_stepped_marker_popover(0, Direction::Next),
        "precondition: the fixture has an annotation"
    );
    let card = view
        .imp()
        .marker_card
        .borrow()
        .clone()
        .expect("the card exists once opened");
    until(Clock::Frame, "the card to open over its chip", || {
        card.is_visible() && card.pointing_to().is_some()
    });
    // A reader resizes some time after opening the card, not within the open's re-pin
    // guard (`REPIN_GUARD_US`, 1.5 s), which holds the scroll value still on purpose.
    drain_for(Clock::Frame, Duration::from_millis(1800));
    let before = read(&view, &card, &pane);
    assert!(
        before.chip.is_some() && before.pointing == before.chip,
        "precondition: the open card points at its on-screen chip: {before:?}"
    );

    crate::window::testkit::resize_mapped_window(&win, to, HEIGHT);
    until(Clock::Frame, "the window to take its new width", || {
        win.width() == to
    });
    // The view re-measures the lines above the viewport in idle passes, so the scroll
    // value and the chip settle after the window has its width.
    let settled = until_stable(
        Clock::Frame,
        Duration::from_secs(5),
        Duration::from_millis(300),
        || (vadj.value(), chip_on_viewport(&view).as_ref().map(tuple)),
    );
    assert!(settled.converged, "the view never settled after the resize");
    let pointed = until_or_for(Clock::Frame, Duration::from_secs(2), || {
        card.is_visible()
            && chip_on_viewport(&view).is_some()
            && card.pointing_to().as_ref().map(tuple) == chip_on_viewport(&view).as_ref().map(tuple)
    });
    let after = read(&view, &card, &pane);
    println!("[17.64 comment card] {scroll:?} {from} -> {to}: before {before:?}, after {after:?}");

    let (Some(chip_before), Some(chip_after)) = (before.chip, after.chip) else {
        panic!("precondition: the chip stays on the viewport through the resize: {after:?}");
    };
    let chip_y = |(_, y, _, _): (i32, i32, i32, i32)| y;
    assert!(
        (to > from && chip_y(chip_after) < chip_y(chip_before))
            || (to < from && chip_y(chip_after) > chip_y(chip_before)),
        "precondition: the resize must move the chip up when widening and down when \
         narrowing, or nothing is tested: before {before:?}, after {after:?}"
    );
    match scroll {
        Scroll::AtTop => assert_eq!(
            after.value, before.value,
            "precondition: with nothing above the viewport the scroll value stays put"
        ),
        Scroll::BelowWrappedText => assert_ne!(
            after.value, before.value,
            "precondition: re-wrapping the text above the viewport moves the scroll value"
        ),
    }
    // The card sits beside the chip it points at, on its usual side or flipped to the
    // other (TDD 17.64): its body must not overlap the chip's column.
    let chip_in_pane = view
        .compute_point(
            &pane,
            &gtk::graphene::Point::new(chip_after.0 as f32, chip_after.1 as f32),
        )
        .expect("the view is inside its pane");
    let beside = after.card.is_some_and(|(cx, _, cw, _)| {
        cx + cw <= chip_in_pane.x() || cx >= chip_in_pane.x() + chip_after.2 as f32
    });
    let failure = if !(card.is_open() && card.is_visible()) {
        Some("the resize closed the comment card")
    } else if !pointed {
        Some("the comment card still points where the chip was")
    } else if !beside {
        Some("the comment card sits over its chip rather than beside it")
    } else {
        None
    };
    destroy_after_tooltip_delay(&win, || view.popdown_marker_popover());
    failure.map(|what| {
        format!(
            "{scroll:?} {from} -> {to}: {what}: chip at x {} in the pane, before {before:?}, \
             after {after:?}",
            chip_in_pane.x()
        )
    })
}

/// Every scroll position's case, failing once with all of them, so a defect in one does
/// not hide whether the other holds.
fn check_both(from: i32, to: i32) {
    let failures: Vec<String> = [Scroll::AtTop, Scroll::BelowWrappedText]
        .into_iter()
        .filter_map(|scroll| resize_and_check(scroll, from, to))
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// **A resize that moves the chip up keeps the comment card up, pointing at the chip's new
/// place (TDD 17.64)** — both with nothing above the viewport, where the reflow fires no
/// `value-changed`, and below wrapped text, where the view moves its scroll value to keep
/// the top line. The chip is in the right margin, so it moves sideways with the width too.
///
/// Mutation: emptying the card's `page-size` hook (`arm_scroll_tracking`) leaves it
/// pointing where the chip was in the at-top case. Re-pointing the hook at another
/// hadjustment property is NOT a mutant: GTK notifies every property on each reconfigure,
/// so `notify::lower` re-anchored the card just as well.
#[gtktest::test]
fn a_resize_that_moves_the_chip_up_re_points_the_comment_card() {
    check_both(NARROW, WIDE);
}

/// **A resize that moves the chip down keeps the comment card up, pointing at the chip's
/// new place (TDD 17.64)**, in both scroll positions.
#[gtktest::test]
fn a_resize_that_moves_the_chip_down_re_points_the_comment_card() {
    check_both(WIDE, NARROW);
}
