//! Height bounds for the annotation card, so a long annotation can never push the card's
//! controls out of reach.
//!
//! The card is a `FloatingCard`: an in-pane overlay child, clipped to the pane it floats
//! over. A long claim or comment used to be a bare wrapping `GtkLabel`, whose minimum
//! height is its whole wrapped text, so a long enough annotation lost the bottom of the
//! card — the Edit / Remove buttons — past the pane's edge, and left nothing to click.
//!
//! Two bounds, at two levels, each answering a different case:
//!
//!  * [`bounded_text`] caps each claim and comment at a few lines and scrolls the rest, so
//!    a row's buttons stay right under its text however long that text is. This is what
//!    keeps the buttons in view without the reader scrolling for them.
//!  * [`fit_to_pane`] wraps the whole list of rows in a scroller with a small minimum, so
//!    when many annotations share one line and their capped rows still outgrow the pane,
//!    the card — which `FloatingCard` never sizes taller than its host — scrolls its rows
//!    rather than being clipped with later rows' buttons past the pane's edge.
//!
//! The two nest, and GTK before 4.20.1 never hands a wheel from an inner scroller to an
//! outer one, so [`RowWheel`] does it: a wheel a capped area cannot use moves the rows.
//!
//! Lines, not pixels: the caps are measured from the label's own font, so they track
//! the theme and the zoom rather than a hard-coded height.

use gtk::glib;
use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

/// The most lines of the annotated (claim) text a card row shows before scrolling. Kept
/// short: the claim is context — the reader can see it in the document beside the card.
pub(super) const CLAIM_MAX_LINES: i32 = 4;

/// The most lines of a comment a card row shows before scrolling.
pub(super) const COMMENT_MAX_LINES: i32 = 10;

/// `label` inside a vertical scroller that grows with the text up to `max_lines` lines
/// of it, then scrolls. Never scrolls horizontally: the label wraps.
///
/// A wheel over it that it cannot use is handed to the rows' scroller through `wheel`
/// (see [`RowWheel`]), shared by every capped area of one card.
pub(super) fn bounded_text(
    label: &gtk::Label,
    max_lines: i32,
    wheel: &RowWheel,
) -> gtk::ScrolledWindow {
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_propagate_natural_height(true);
    scroller.set_max_content_height(line_px(label) * max_lines);
    scroller.set_child(Some(label));
    wheel.install_area(&scroller);
    scroller
}

/// The card's wheel routing: hands a wheel over a row's capped text to the scroller
/// holding the rows when the text cannot use it.
///
/// # Why
///
/// On GTK 4.6 a `GtkScrolledWindow`'s bubble-phase scroll handler returns
/// `GDK_EVENT_STOP` on every path — at its end, and even with nothing to scroll
/// (`scroll_controller_scroll`, gtkscrolledwindow.c:1461-1472, 4.6.9). So a wheel over a
/// row's claim or comment never reached the rows' scroller, and with the pointer over the
/// rows — most of the card — the list did not move.
///
/// GTK from 4.20.1 hands the wheel on itself (see [`HAND_OFF_SINCE`]): the bubble handler
/// propagates when the scroller cannot scroll at all, or when the event overshoots its
/// end; a touchpad sequence is held by the inner scroller until its overshoot reaches
/// the maximum and then taken, and held, by the outer one. There this does nothing, so
/// GTK's own units, overshoot and kinetic glide are what the reader gets.
///
/// # The rule, mirrored from that GTK
///
/// Per event: a capped area that can still move the way the wheel turns keeps it; one at
/// that end, or with nothing to scroll, gives it to the rows, which move by GTK's own
/// step. Once a TOUCHPAD sequence has gone to the rows it stays there until it ends,
/// because moving the rows slides a different area under the pointer, and that area
/// taking the rest of the fling would stop the list dead mid-gesture. A wheel has no
/// sequence, so each detent is decided on its own, as GTK does.
///
/// Holds no widget: it is captured by controllers on the scrollers it routes.
#[derive(Clone, Default)]
pub(super) struct RowWheel(Rc<Cell<bool>>);

/// Where one wheel event over a capped area goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    /// The area scrolls itself, by GTK's own handling.
    Area,
    /// The rows' scroller takes it.
    Rows,
}

/// The routing decision as plain data: `held` is whether this touchpad sequence already
/// went to the rows.
fn route(held: bool, area_can_move: bool) -> Route {
    if held || !area_can_move {
        Route::Rows
    } else {
        Route::Area
    }
}

/// Whether the rows hold the sequence after this event: from the first smooth (touchpad)
/// event routed to them until the sequence ends.
fn holds_after(held: bool, to: Route, smooth: bool) -> bool {
    held || (to == Route::Rows && smooth)
}

/// Whether a scroller at `value` in `[lower, upper - page]` can move by a wheel turning
/// `dy` (positive is down). False both at the end it is turned towards and when the
/// content fits, which is most comments. Half a pixel of slack: GTK stores fractional
/// values and its clamp lands within rounding of the end.
fn can_move(value: f64, lower: f64, upper: f64, page: f64, dy: f64) -> bool {
    const SLACK: f64 = 0.5;
    if dy > 0.0 {
        value < upper - page - SLACK
    } else {
        dy < 0.0 && value > lower + SLACK
    }
}

/// Whether the running GTK hands a wheel from an inner scroller to an outer one itself.
///
/// Asked of the RUNTIME, not the build: the crate's `v4_6` floor says what we may call,
/// not what is loaded.
fn toolkit_hands_off() -> bool {
    hands_off_since(
        gtk::major_version(),
        gtk::minor_version(),
        gtk::micro_version(),
    )
}

/// The version comparison itself, separated so it is tested at the boundary.
fn hands_off_since(major: u32, minor: u32, micro: u32) -> bool {
    (major, minor, micro) >= HAND_OFF_SINCE
}

/// The first GTK whose scrolled window propagates a wheel it cannot use at its END:
/// 4.20.1, GTK's "scrolledwindow: Improve behavior if nested in other scrollables"
/// (2025-02-04), which added the edge and overshoot propagation and the touchpad hold.
/// From 4.10.0 ("scrolledWindow: Propagate event for no scrolling", 2023-01-25) a
/// scroller with no scrollbar showing already propagates, but one at its end still
/// stops the wheel, so this stays installed there; it takes such an event first and
/// consumes it, so the two never both move the rows.
const HAND_OFF_SINCE: (u32, u32, u32) = (4, 20, 1);

impl RowWheel {
    /// Route `area`'s wheel events through this card.
    fn install_area(&self, area: &gtk::ScrolledWindow) {
        if toolkit_hands_off() {
            return;
        }
        let controller = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        // CAPTURE, added after the scroller is built: `add_controller` prepends and a
        // phase runs head-first, so this decides before the scroller's own capture
        // controller, which takes a touchpad sequence itself (GTK4Rs/AP-302, GTK4Rs/AP-326).
        // The rows' scroller is an ancestor, so ITS capture controller has already seen
        // the event and keeps its kinetic history: a touchpad fling handed to the rows
        // still glides as GTK would glide it.
        controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        let wheel = self.clone();
        controller.connect_scroll(glib::clone!(
            #[weak]
            area,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |controller, _dx, dy| wheel.on_area_scroll(&area, controller, dy)
        ));
        area.add_controller(controller);
    }

    /// Watch `rows` for touchpad sequence boundaries, which end the rows' hold. On the
    /// rows' scroller, not on an area: a sequence that moved the rows ends over whichever
    /// area — or gap between areas — is under the pointer by then, and the rows see
    /// every event inside them. This is the boundary GTK's own capture controller on
    /// that scroller keeps.
    fn install_rows(&self, rows: &gtk::ScrolledWindow) {
        if toolkit_hands_off() {
            return;
        }
        let controller = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        let held = self.0.clone();
        controller.connect_scroll_begin(glib::clone!(
            #[strong]
            held,
            move |_| held.set(false)
        ));
        controller.connect_scroll_end(move |_| held.set(false));
        rows.add_controller(controller);
    }

    fn on_area_scroll(
        &self,
        area: &gtk::ScrolledWindow,
        controller: &gtk::EventControllerScroll,
        dy: f64,
    ) -> glib::Propagation {
        // Shift swaps the axes (gtkscrolledwindow.c:1404-1416), and nothing in the card
        // scrolls sideways: leave it to GTK, as before.
        if controller
            .current_event_state()
            .contains(gtk::gdk::ModifierType::SHIFT_MASK)
        {
            return glib::Propagation::Proceed;
        }
        let Some(rows) = area
            .parent()
            .and_then(|p| p.ancestor(gtk::ScrolledWindow::static_type()))
            .and_downcast::<gtk::ScrolledWindow>()
        else {
            return glib::Propagation::Proceed;
        };
        let smooth = controller
            .current_event()
            .and_then(|event| event.downcast::<gtk::gdk::ScrollEvent>().ok())
            .is_some_and(|event| event.direction() == gtk::gdk::ScrollDirection::Smooth);
        // From 4.8 GTK scales a touchpad event by the event's unit, which has no binding
        // below the crate's `v4_8` feature. Rather than move the rows by a guessed step,
        // such an event is left to GTK there, as before this existed.
        if smooth && gtk::check_version(4, 8, 0).is_none() {
            return glib::Propagation::Proceed;
        }
        let adj = area.vadjustment();
        let held = self.0.get();
        let to = route(
            held,
            can_move(adj.value(), adj.lower(), adj.upper(), adj.page_size(), dy),
        );
        self.0.set(holds_after(held, to, smooth));
        match to {
            Route::Area => glib::Propagation::Proceed,
            Route::Rows => {
                let step = crate::window::wheelcoalesce::event_step(&rows, controller);
                let vadj = rows.vadjustment();
                crate::saferizer::scrollpos::jump(&vadj, vadj.value() + dy * step);
                glib::Propagation::Stop
            }
        }
    }
}

/// `body` inside a vertical scroller whose natural height is the body's and whose
/// minimum is small: natural size where the pane has room, scrolling where the card is
/// given less (GTK4Rs/AP-86's wrapper shape). `wheel` is the one its rows' capped text
/// areas were built with, so a wheel they cannot use moves this scroller.
pub(super) fn fit_to_pane(body: &impl IsA<gtk::Widget>, wheel: &RowWheel) -> gtk::ScrolledWindow {
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_propagate_natural_width(true);
    scroller.set_propagate_natural_height(true);
    // An explicit viewport so its vertical scroll policy can be NATURAL. The default
    // (MINIMUM) lays the rows out at their minimum height, and a capped text area's
    // minimum is a sliver, so every comment shrank to a few lines once the rows scrolled
    // (GTK4Rs/AP-358).
    let viewport = gtk::Viewport::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    viewport.set_vscroll_policy(gtk::ScrollablePolicy::Natural);
    viewport.set_child(Some(body));
    scroller.set_child(Some(&viewport));
    wheel.install_rows(&scroller);
    scroller
}

/// One line of `label`'s text, in pixels, in the font it will be drawn in.
fn line_px(label: &gtk::Label) -> i32 {
    let (_, height) = label.create_pango_layout(Some("0")).pixel_size();
    height.max(1)
}

#[cfg(test)]
mod decision_tests {
    use super::*;

    /// The rule (TDD 17.66): an area that can move keeps the wheel; one that cannot —
    /// at the end it is turned towards, or with nothing to scroll — gives it to the
    /// rows; and a touchpad sequence the rows took stays with them.
    #[test]
    fn an_area_keeps_the_wheel_only_while_it_can_move() {
        assert_eq!(route(false, true), Route::Area);
        assert_eq!(route(false, false), Route::Rows);
        assert_eq!(
            route(true, true),
            Route::Rows,
            "a held sequence stays with the rows"
        );
        assert_eq!(route(true, false), Route::Rows);
    }

    /// Only a smooth (touchpad) event starts a hold: a wheel detent has no sequence to
    /// hold, so GTK decides each one on its own and so does this.
    #[test]
    fn only_a_touchpad_sequence_handed_to_the_rows_is_held() {
        assert!(holds_after(false, Route::Rows, true));
        assert!(!holds_after(false, Route::Rows, false));
        assert!(!holds_after(false, Route::Area, true));
        assert!(holds_after(true, Route::Rows, false));
    }

    /// Both ends, and the content-fits case, at a fractional value within GTK's clamp.
    #[test]
    fn an_area_can_move_only_away_from_the_end_it_is_at() {
        // value, lower, upper, page
        let top = (0.0, 0.0, 908.0, 190.0);
        let middle = (300.25, 0.0, 908.0, 190.0);
        let bottom = (717.8, 0.0, 908.0, 190.0);
        let fits = (0.0, 0.0, 46.0, 46.0);
        let ok = |(v, l, u, p): (f64, f64, f64, f64), dy| can_move(v, l, u, p, dy);
        assert!(ok(top, 1.0) && !ok(top, -1.0));
        assert!(ok(middle, 1.0) && ok(middle, -1.0));
        assert!(!ok(bottom, 1.0) && ok(bottom, -1.0));
        assert!(!ok(fits, 1.0) && !ok(fits, -1.0));
        assert!(!ok(middle, 0.0), "no turn, no move");
    }

    /// The runtime gate includes the micro component: 4.20.0 still stops a wheel at
    /// the inner scroller's end, 4.20.1 hands it on.
    #[test]
    fn the_hand_off_is_left_to_gtk_from_4_20_1() {
        assert!(!hands_off_since(4, 6, 9));
        assert!(!hands_off_since(4, 10, 0));
        assert!(!hands_off_since(4, 20, 0));
        assert!(hands_off_since(4, 20, 1));
        assert!(hands_off_since(4, 22, 4));
        assert!(hands_off_since(4, 24, 1));
        assert!(hands_off_since(5, 0, 0));
    }
}

/// Live-display tests for the card's height bounds and its wheel. Run with `cargo test
/// --features gtk-integration-tests`, under Xvfb if headless.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;
    use crate::annotations::Direction;
    use crate::codeview::card::AnnotationCard;
    use crate::codeview::CodePreviewView;
    use gtk::subclass::prelude::*;

    /// The first descendant `GtkButton` labelled `label`.
    fn find_button(root: &impl IsA<gtk::Widget>, label: &str) -> Option<gtk::Button> {
        let mut child = root.as_ref().first_child();
        while let Some(c) = child {
            if let Ok(b) = c.clone().downcast::<gtk::Button>() {
                if b.label().as_deref() == Some(label) {
                    return Some(b);
                }
            }
            if let Some(found) = find_button(&c, label) {
                return Some(found);
            }
            child = c.next_sibling();
        }
        None
    }

    /// How many descendant `GtkButton`s are labelled `label`.
    fn count_buttons(root: &impl IsA<gtk::Widget>, label: &str) -> usize {
        let mut count = 0;
        let mut child = root.as_ref().first_child();
        while let Some(c) = child {
            let hit = c
                .clone()
                .downcast::<gtk::Button>()
                .is_ok_and(|b| b.label().as_deref() == Some(label));
            count += usize::from(hit) + count_buttons(&c, label);
            child = c.next_sibling();
        }
        count
    }

    /// Every descendant capped scroller that has text to scroll yet is shorter than its
    /// cap, as `(height, cap)`. A capped area with more text than fits should be exactly
    /// as tall as its cap; shorter means its container laid it out at its minimum.
    fn squeezed_scrollers(root: &impl IsA<gtk::Widget>) -> Vec<(i32, i32)> {
        let mut found = Vec::new();
        let mut child = root.as_ref().first_child();
        while let Some(c) = child {
            if let Ok(sw) = c.clone().downcast::<gtk::ScrolledWindow>() {
                let adj = sw.vadjustment();
                let cap = sw.max_content_height();
                if cap > 0 && adj.upper() > adj.page_size() + 1.0 && sw.height() + 1 < cap {
                    found.push((sw.height(), cap));
                }
            }
            found.extend(squeezed_scrollers(&c));
            child = c.next_sibling();
        }
        found
    }

    fn view_of(pane: gtk::Widget) -> CodePreviewView {
        pane.downcast::<gtk::Overlay>()
            .expect("preview pane is a GtkOverlay")
            .child()
            .and_then(|c| c.downcast::<gtk::ScrolledWindow>().ok())
            .expect("overlay wraps the scroller")
            .child()
            .and_then(|c| c.downcast::<CodePreviewView>().ok())
            .expect("scroller holds the CodePreviewView")
    }

    /// A presented 700-wide window `height` tall rendering `md`, with the card open on
    /// its first annotation. Returns `(window, card)`; keep the window alive for the test.
    fn open_card_on(md: &str, height: i32) -> (gtk::Window, AnnotationCard) {
        let pane = crate::preview::render(
            None,
            md,
            None,
            1.0,
            false,
            &crate::fold::FoldState::default(),
        );
        let view = view_of(pane.clone());
        view.set_annotation_sink(std::rc::Rc::new(|_| {}));
        let win = gtk::Window::new();
        win.set_default_size(700, height);
        win.set_child(Some(&pane));
        win.present();
        crate::testpump::until(crate::testpump::Clock::Frame, "the chip to paint", || {
            !view.imp().marker_hitboxes.borrow().is_empty()
        });
        assert!(
            view.open_stepped_marker_popover(0, Direction::Next),
            "precondition: the fixture has an annotation"
        );
        let card = || view.imp().marker_card.borrow().clone();
        crate::testpump::until(crate::testpump::Clock::Frame, "the card to lay out", || {
            card().is_some_and(|c| c.is_mapped() && c.height() > 0)
        });
        (win, card().expect("the card is open"))
    }

    /// The card's height and its pane's, once its first Edit button is allocated.
    fn card_and_pane_heights(card: &AnnotationCard) -> (i32, i32) {
        let edit = find_button(card, "Edit").expect("the card offers Edit");
        crate::testpump::until(
            crate::testpump::Clock::Frame,
            "Edit to be allocated",
            || edit.is_mapped() && edit.height() > 0,
        );
        let bounds = edit
            .compute_bounds(card)
            .expect("Edit shares the card's tree");
        assert!(
            bounds.y() >= 0.0 && bounds.y() + bounds.height() <= card.height() as f32,
            "Edit at y {}..{} lies outside the card's height {}",
            bounds.y(),
            bounds.y() + bounds.height(),
            card.height()
        );
        let pane = card.parent().expect("the card floats over its pane");
        (card.height(), pane.height())
    }

    /// **A long annotation keeps its Edit / Remove buttons on the card** (TDD 17.60).
    ///
    /// Claim and comment together are thousands of pixels tall when wrapped at the card's
    /// width — taller than any pane — so without the bounds the pane clips the card and
    /// the buttons, below the text, are allocated outside it. Asserted on allocations:
    /// the button must lie inside the card, and the card inside its pane.
    #[gtktest::test]
    fn a_long_annotation_keeps_its_buttons_on_the_card() {
        // The claim is long enough to overflow the card but short enough that its chip
        // stays inside the window (a chip off the viewport hides the card, TDD 17.48);
        // the comment is drawn only on the card, so it can be as long as it likes.
        let claim = "word ".repeat(300);
        let comment = "word ".repeat(1500);
        let (win, card) = open_card_on(
            &format!("Intro.\n\nThe {{=={claim}==}}{{>>{comment}<<}} ends.\n"),
            900,
        );
        let (card_h, pane_h) = card_and_pane_heights(&card);
        assert!(
            card_h <= pane_h,
            "the card ({card_h} px) is taller than its pane ({pane_h} px)"
        );
        crate::testpump::destroy_after_tooltip_delay(&win, || card.dismiss());
    }

    /// **Many long annotations on one line still fit a short pane** (TDD 17.60).
    ///
    /// Annotations on one line share one card, one row each. Every row is capped, but
    /// enough of them still add up to more than the pane; the card must scroll its rows
    /// inside the pane rather than be clipped by it, which left the later rows' Edit and
    /// Remove past the pane's edge with nothing to scroll. A short window, because the
    /// defect needs the rows to outgrow the pane but not the monitor.
    ///
    /// Mutation check: sizing the card at its natural height in `size_within` fails the
    /// pane assertion here.
    #[gtktest::test]
    fn many_long_annotations_on_one_line_still_fit_a_short_pane() {
        let comment = "word ".repeat(400);
        let line: String = (0..8)
            .map(|n| format!("{{==claim {n}==}}{{>>{comment}<<}} "))
            .collect();
        let (win, card) = open_card_on(&format!("Intro.\n\n{line}\n"), 420);
        let rows = count_buttons(&card, "Remove");
        assert!(
            rows >= 2,
            "precondition: the line's annotations share one card (found {rows} rows)"
        );
        let (card_h, pane_h) = card_and_pane_heights(&card);
        let squeezed = squeezed_scrollers(&card);
        assert!(
            squeezed.is_empty(),
            "scrolling text areas shorter than their cap (height, cap): {squeezed:?}"
        );
        assert!(
            card_h <= pane_h,
            "the card ({card_h} px) does not fit its pane ({pane_h} px)"
        );
        let rows_scroller = rows_scroller(&card);
        let vadj = rows_scroller.vadjustment();
        assert!(
            vadj.upper() > vadj.page_size(),
            "the rows must scroll inside the capped card: upper {} page {}",
            vadj.upper(),
            vadj.page_size()
        );
        crate::testpump::destroy_after_tooltip_delay(&win, || card.dismiss());
    }

    /// The scroller [`fit_to_pane`] wraps the rows in: the one whose child is a viewport.
    fn rows_scroller(card: &AnnotationCard) -> gtk::ScrolledWindow {
        let mut stack = vec![card.clone().upcast::<gtk::Widget>()];
        while let Some(w) = stack.pop() {
            if let Ok(sw) = w.clone().downcast::<gtk::ScrolledWindow>() {
                if sw
                    .child()
                    .and_downcast::<gtk::Viewport>()
                    .is_some_and(|v| v.vscroll_policy() == gtk::ScrollablePolicy::Natural)
                {
                    return sw;
                }
            }
            let mut c = w.first_child();
            while let Some(child) = c {
                c = child.next_sibling();
                stack.push(child);
            }
        }
        panic!("the card wraps its rows in fit_to_pane's scroller");
    }

    /// Deliver one wheel detent of `dy` over `target` the way GTK 4.6.9 dispatches it:
    /// every `GtkEventControllerScroll` on the path, capture phase from the toplevel
    /// down, then target, then bubble back up, each widget's controllers in list order,
    /// and the first one that reports the event handled ends it (`gtk_widget_run_controllers`,
    /// gtkwidget.c:4523-4592; the list order is the dispatch order, GTK4Rs/AP-302).
    /// `#[gtktest::test]` cannot synthesise a wheel event, so this emits each
    /// controller's `scroll` with GTK's real handlers behind it.
    ///
    /// A DISCRETE wheel only: there is no current event, so nothing is smooth. The
    /// touchpad hold is tested on its own below, and a real touchpad is MANUAL-TEST's.
    fn wheel_over(target: &gtk::Widget, dy: f64) {
        let run = |widget: &gtk::Widget, phase| {
            widget
                .observe_controllers()
                .into_iter()
                .filter_map(|c| c.ok().and_downcast::<gtk::EventControllerScroll>())
                .filter(|c| c.propagation_phase() == phase)
                .filter(|c| {
                    c.flags()
                        .contains(gtk::EventControllerScrollFlags::VERTICAL)
                })
                .any(|c| c.emit_by_name::<bool>("scroll", &[&0.0f64, &dy]))
        };
        let mut path = Vec::new();
        let mut at = Some(target.clone());
        while let Some(w) = at {
            at = w.parent();
            path.push(w);
        }
        let handled = path
            .iter()
            .rev()
            .any(|w| run(w, gtk::PropagationPhase::Capture))
            || run(target, gtk::PropagationPhase::Target)
            || path.iter().any(|w| run(w, gtk::PropagationPhase::Bubble));
        assert!(handled, "precondition: something on the path takes a wheel");
    }

    /// The capped text areas inside `rows`, in document order.
    fn text_areas(rows: &gtk::ScrolledWindow) -> Vec<gtk::ScrolledWindow> {
        fn walk(w: &gtk::Widget, rows: &gtk::ScrolledWindow, out: &mut Vec<gtk::ScrolledWindow>) {
            if let Ok(sw) = w.clone().downcast::<gtk::ScrolledWindow>() {
                if &sw != rows && sw.max_content_height() > 0 {
                    out.push(sw);
                }
            }
            let mut c = w.first_child();
            while let Some(child) = c {
                walk(&child, rows, out);
                c = child.next_sibling();
            }
        }
        let mut out = Vec::new();
        walk(rows.upcast_ref(), rows, &mut out);
        out
    }

    /// The first area inside the rows' visible page that `fits` (has nothing to scroll)
    /// or does not.
    fn visible_area(rows: &gtk::ScrolledWindow, fits: bool) -> gtk::ScrolledWindow {
        text_areas(rows)
            .into_iter()
            .find(|a| {
                let adj = a.vadjustment();
                let mid = a
                    .compute_point(
                        rows,
                        &gtk::graphene::Point::new(0.0, a.height() as f32 / 2.0),
                    )
                    .map_or(-1.0, |p| p.y());
                (adj.upper() <= adj.page_size() + 1.0) == fits
                    && mid > 0.0
                    && mid < rows.height() as f32
            })
            .expect("precondition: the fixture shows such an area")
    }

    /// Eight long annotations on one line in a short window, so the rows overflow, each
    /// with a short claim (nothing to scroll) and a long comment (scrolls).
    fn open_overflowing_card() -> (gtk::Window, AnnotationCard, gtk::ScrolledWindow) {
        let comment = "word ".repeat(400);
        let line: String = (0..8)
            .map(|n| format!("{{==claim {n}==}}{{>>{comment}<<}} "))
            .collect();
        let (win, card) = open_card_on(&format!("Intro.\n\n{line}\n"), 420);
        let _ = card_and_pane_heights(&card);
        let rows = rows_scroller(&card);
        let vadj = rows.vadjustment();
        assert!(
            vadj.upper() > vadj.page_size() + 100.0,
            "precondition: the rows overflow the card"
        );
        (win, card, rows)
    }

    /// **The wheel over a row's text that has nothing to scroll moves the rows**
    /// (TDD 17.66). Before the fix the claim's scroller took the wheel and the rows
    /// never moved — measured with a real XTEST wheel on Xvfb, 40 detents: 0 px.
    #[gtktest::test]
    fn a_wheel_over_text_with_nothing_to_scroll_moves_the_rows() {
        let (win, card, rows) = open_overflowing_card();
        let claim = visible_area(&rows, true);
        let before = rows.vadjustment().value();
        wheel_over(claim.upcast_ref(), 1.0);
        let moved = rows.vadjustment().value() - before;
        let step = rows.vadjustment().page_size().powf(2.0 / 3.0);
        assert!(
            (moved - step).abs() < 1.0,
            "one detent over the claim moved the rows {moved} px, GTK's step is {step} px"
        );
        crate::testpump::destroy_after_tooltip_delay(&win, || card.dismiss());
    }

    /// **The wheel over a long comment scrolls the comment to its end, then the rows**
    /// (TDD 17.66), and back up the same way: the comment first, the rows when it is at
    /// its top.
    #[gtktest::test]
    fn a_wheel_over_a_long_comment_scrolls_it_to_its_end_then_the_rows() {
        let (win, card, rows) = open_overflowing_card();
        let comment = visible_area(&rows, false);
        let (cadj, radj) = (comment.vadjustment(), rows.vadjustment());
        let mut detents = 0;
        while cadj.value() < cadj.upper() - cadj.page_size() - 0.5 {
            wheel_over(comment.upcast_ref(), 1.0);
            detents += 1;
            assert!(detents < 100, "the comment never reached its end");
            // Only while the comment is still short of its end: from 4.20.1 GTK hands
            // on the overshoot of the detent that reaches it.
            let short_of_end = cadj.value() < cadj.upper() - cadj.page_size() - 0.5;
            assert!(
                !short_of_end || radj.value() == 0.0,
                "the rows moved before the comment reached its end"
            );
        }
        assert!(
            detents > 1,
            "precondition: the comment takes several detents"
        );
        wheel_over(comment.upcast_ref(), 1.0);
        let down = radj.value();
        assert!(
            down > 1.0,
            "a detent past the comment's end left the rows at {down}"
        );

        // A reader pauses before turning back, and from 4.20.1 GTK springs an
        // overshoot back over frames; let it, or the turn back starts past the end.
        crate::testpump::drain_for(
            crate::testpump::Clock::Frame,
            std::time::Duration::from_millis(500),
        );
        let (at_end, down) = (cadj.value(), radj.value());
        wheel_over(comment.upcast_ref(), -1.0);
        assert!(
            cadj.value() < at_end,
            "turning back scrolls the comment first"
        );
        assert_eq!(radj.value(), down, "and leaves the rows where they were");
        crate::testpump::destroy_after_tooltip_delay(&win, || card.dismiss());
    }

    /// **A touchpad sequence the rows took is released when it ends** (TDD 17.66). The
    /// hold keeps a fling on the rows when it slides another comment under the pointer;
    /// a hold that outlived its sequence would send the next gesture over a comment to
    /// the rows instead. Emitted on the rows' boundary watcher, since a test cannot
    /// deliver a smooth event.
    #[gtktest::test]
    fn the_rows_release_a_touchpad_sequence_when_it_ends() {
        if toolkit_hands_off() {
            println!("SKIPPED [17.66]: GTK hands the wheel off itself; nothing installed");
            return;
        }
        let wheel = RowWheel::default();
        let rows = fit_to_pane(&gtk::Box::new(gtk::Orientation::Vertical, 0), &wheel);
        let watcher = rows
            .observe_controllers()
            .into_iter()
            .filter_map(|c| c.ok().and_downcast::<gtk::EventControllerScroll>())
            .find(|c| c.flags() == gtk::EventControllerScrollFlags::VERTICAL)
            .expect("fit_to_pane installs the sequence watcher");
        for signal in ["scroll-end", "scroll-begin"] {
            wheel.0.set(true);
            watcher.emit_by_name::<()>(signal, &[]);
            assert!(!wheel.0.get(), "{signal} must release the rows' hold");
        }
    }
}
