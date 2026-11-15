//! A caret the reader moves while Ctrl+Home / Ctrl+End is still travelling retires the
//! travel, and the viewport stays on the caret (TDD 9.33).
//!
//! # The defect
//!
//! The buffer-ends keys scroll by ANIMATING the adjustment, GTK's own scroll first and
//! [`super::wire_buffer_ends_scroll`]'s re-issue after it, and on a large document still
//! being laid out the two together are a travel the reader can watch for seconds. A
//! click during it moved the caret and nothing else: the animation is a frame-clock tick
//! on the adjustment, aimed at the end, and it ran on past the line the reader had just
//! clicked. MEASURED on Windows (GTK 4.22.4, the 20 000-line fixture): clicked at line 43
//! with the view at 880 px, the travel carried on to 1 250 px and parked there, top line
//! 46, caret off screen for good. The deferred re-issue's caret gate abandoned the jump
//! correctly; it was GTK's own scroll in flight that nothing retired. The app on Linux
//! did not show it, its travel being over in under 0.1 s; the tests below hold the
//! travel open so they fail without the repair on every platform.
//!
//! # The repair
//!
//! A buffer-ends key arms a watch on the caret. The first time the caret is SET away from
//! the end the key asked for, the watch disarms itself and, if the caret is on screen
//! (where a click puts it), freezes the travel there: a plain adjustment write ends the
//! animation where it has reached ([`crate::saferizer::scrollpos::jump`]), and a scroll
//! to the caret replaces whatever scroll GTK still has queued for the end (a new
//! `scroll_to_mark` frees the pending one), deferred and re-issued like any navigation so
//! it holds once the layout is valid.
//!
//! A caret set somewhere OFF screen is left alone: whoever put it there (Go To Line, Find,
//! the opposite key) issues its own scroll, and the deferred re-issue's licence test
//! already refuses to drag the reader to it ([`super::caret_still_at_requested_end`]).
//!
//! # Cost
//!
//! `mark-set` is a hot path (Hot-path CAM row 3): GtkSourceView's highlighter sets
//! thousands of marks on a large document. The watch is connected only from a
//! buffer-ends key until the caret's first move after it, and returns after one mark
//! identity test for every other mark.

use crate::saferizer::qdata_key::QdataKey;
use gtk::glib;
use gtk::prelude::*;

/// The watch armed on a view by its last buffer-ends key: the `mark-set` handler and the
/// buffer it is connected to, held weakly because the view may be given another buffer.
struct Watch {
    buffer: glib::WeakRef<gtk::TextBuffer>,
    handler: glib::SignalHandlerId,
}

/// Where a view keeps its armed watch. One per view: a new buffer-ends key replaces it.
const WATCH: QdataKey<Watch> = QdataKey::new("scrib-buffer-ends-watch");

/// Arm the watch for a buffer-ends key on `view`, aimed at the end (`to_end`) or the
/// start. Called from the key's `move-cursor`, before GTK's own handler moves the caret.
pub(super) fn arm(view: &gtk::TextView, to_end: bool) {
    disarm(view);
    let buffer = view.buffer();
    let weak_view = view.downgrade();
    let handler = buffer.connect_mark_set(move |buffer, iter, mark| {
        if mark != &buffer.get_insert() {
            return; // the hot path: any other mark, one identity test
        }
        if super::caret_still_at_requested_end(to_end, iter.is_end(), iter.offset()) {
            return; // the key's own move, or a move that stayed at that end
        }
        let Some(view) = weak_view.upgrade() else {
            return;
        };
        // First, so nothing below (a mark the scroll creates) re-enters this handler.
        disarm(&view);
        if view.buffer() == *buffer {
            freeze_on_caret(&view);
        }
    });
    WATCH.set(
        view,
        Watch {
            buffer: buffer.downgrade(),
            handler,
        },
    );
}

/// Disconnect `view`'s armed watch, if it has one.
fn disarm(view: &gtk::TextView) {
    let Some(Watch { buffer, handler }) = WATCH.steal(view) else {
        return;
    };
    if let Some(buffer) = buffer.upgrade() {
        buffer.disconnect(handler);
    }
}

/// End the travel where it has reached and keep the caret in view, if the caret is on
/// screen; otherwise leave the viewport to whoever moved the caret.
fn freeze_on_caret(view: &gtk::TextView) {
    let Some(adjustment) = view.vadjustment() else {
        return;
    };
    let buffer = view.buffer();
    let insert = buffer.get_insert();
    let (line_y, line_height) = view.line_yrange(&buffer.iter_at_mark(&insert));
    let visible = view.visible_rect();
    if !line_shown(
        line_y,
        line_height,
        visible.y(),
        visible.y() + visible.height(),
    ) {
        return;
    }
    // The write ends the animation in flight: `set_value` always calls
    // `gtk_adjustment_end_updating`, even when the value is unchanged.
    crate::saferizer::scrollpos::jump(&adjustment, adjustment.value());
    #[expect(
        clippy::disallowed_methods,
        reason = "the pane being scrolled is the one the caret moved in, by no more than \
                  keeping that caret on screen; it moves no other pane's lead"
    )]
    super::scroll_to_mark_when_ready(view, &insert, 0.0, false, 0.0, 0.0);
}

/// Whether a line laid out at `line_y` with `line_height` shows within `[top, bottom)`.
///
/// A line past GTK's validation frontier reports the frontier as its `y` and a height of
/// 0, which is no evidence it is on screen, so a zero height never counts as shown.
fn line_shown(line_y: i32, line_height: i32, top: i32, bottom: i32) -> bool {
    line_height > 0 && line_y < bottom && line_y + line_height > top
}

#[cfg(test)]
mod decision_tests {
    use super::line_shown;

    #[test]
    fn a_line_inside_the_viewport_is_shown() {
        assert!(line_shown(300, 20, 0, 600));
    }

    #[test]
    fn a_line_partly_inside_either_edge_is_shown() {
        assert!(line_shown(-10, 20, 0, 600));
        assert!(line_shown(590, 20, 0, 600));
    }

    #[test]
    fn a_line_wholly_above_or_below_is_not_shown() {
        assert!(!line_shown(-20, 20, 0, 600));
        assert!(!line_shown(600, 20, 0, 600));
    }

    #[test]
    fn a_line_past_the_validation_frontier_is_not_shown_even_at_a_visible_y() {
        assert!(!line_shown(300, 0, 0, 600));
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use crate::testpump::{drain_for, until_for, Clock};
    use gtk::glib;
    use gtk::prelude::*;
    use std::time::{Duration, Instant};

    /// How long the travel may take to show its first movement before the run is
    /// declared unable to evidence anything.
    const FIRST_STEP_DEADLINE: Duration = Duration::from_secs(2);

    /// Long past GTK's ~200 ms scroll animation, so a travel the click failed to retire
    /// has had every chance to carry the viewport off the caret.
    const AFTER_CLICK: Duration = Duration::from_millis(1_500);

    /// Pump real time until `adjustment` has moved off `start`, the travel's first
    /// visible step. Time, not turns: the animation is paced by the frame clock
    /// (GTK4Rs/AP-261).
    fn pump_until_moved(adjustment: &gtk::Adjustment, start: f64) {
        let ctx = glib::MainContext::default();
        let t0 = Instant::now();
        while adjustment.value() == start && t0.elapsed() < FIRST_STEP_DEADLINE {
            ctx.iteration(false);
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// Click the first line wholly on screen, as far as the buffer and the viewport see
    /// it: GTK's press handler sets the caret there (`gtk_text_buffer_select_range`,
    /// one `mark-set` on the insert mark) and scrolls nothing. The top line, not a middle
    /// one, so any further travel down puts it off screen. Returns that line.
    fn click_top_line(view: &gtk::TextView) -> i32 {
        let visible = view.visible_rect();
        let (mut iter, line_top) = view.line_at_y(visible.y());
        if line_top < visible.y() {
            iter.forward_line();
        }
        view.buffer().place_cursor(&iter);
        iter.line()
    }

    /// Whether the caret's line shows in `view`'s viewport.
    fn caret_shown(view: &gtk::TextView) -> bool {
        let buffer = view.buffer();
        let (y, h) = view.line_yrange(&buffer.iter_at_mark(&buffer.get_insert()));
        let visible = view.visible_rect();
        super::line_shown(y, h, visible.y(), visible.y() + visible.height())
    }

    /// The first line on screen, for the failure message.
    fn top_line(view: &gtk::TextView) -> i32 {
        view.line_at_y(view.visible_rect().y()).0.line()
    }

    /// **A click while Ctrl+End is travelling leaves the caret on screen (TDD 9.33).**
    ///
    /// The deterministic form: a document already laid out, so the travel is GTK's own
    /// ~200 ms animation and nothing else can end it early on any platform. The click
    /// lands at its first visible step. Unretired, the animation carries the viewport
    /// on to the end and the clicked line leaves the screen.
    ///
    /// Mutation checks: not arming the watch fails this and the test below; deleting
    /// `freeze_on_caret`'s `jump` alone fails this.
    #[gtktest::test]
    fn a_click_during_a_buffer_ends_travel_keeps_the_caret_on_screen() {
        let _motion = crate::testmotion::AnimatedScrolls::pin();
        let body: String = (0..4_000)
            .map(|i| format!("Line {i} — {}\n", "lorem ipsum dolor sit amet ".repeat(4)))
            .collect();
        let (_buffer, view) = crate::window::build_tab_editor(&body);
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_child(Some(&view));
        let window = gtk::Window::new();
        window.set_default_size(800, 600);
        window.set_child(Some(&scroller));
        window.present();
        let adjustment = scroller.vadjustment();
        {
            // Laid out: the range has stopped changing at a document many pages tall.
            let adjustment = adjustment.clone();
            let mut last = f64::NAN;
            let mut unchanged = 0u32;
            until_for(
                Clock::Idle,
                Duration::from_secs(30),
                "the document to be laid out",
                move || {
                    let upper = adjustment.upper();
                    unchanged = if upper == last { unchanged + 1 } else { 0 };
                    last = upper;
                    adjustment.page_size() > 0.0
                        && upper > adjustment.page_size() * 20.0
                        && unchanged >= 200
                },
            );
        }
        let view: gtk::TextView = view.upcast();
        let start = adjustment.value();

        view.emit_move_cursor(gtk::MovementStep::BufferEnds, 1, false);
        pump_until_moved(&adjustment, start);
        let (at_click, bottom) = (
            adjustment.value(),
            adjustment.upper() - adjustment.page_size(),
        );
        assert!(
            at_click > start && at_click < bottom - 1.0,
            "precondition: the click must land while the travel is under way, moved off \
             {start:.0} and short of {bottom:.0}, but the view reads {at_click:.0}"
        );
        let clicked = click_top_line(&view);
        assert!(
            caret_shown(&view),
            "precondition: the clicked line is on screen"
        );

        drain_for(Clock::Frame, AFTER_CLICK);
        let (shown, top, value) = (caret_shown(&view), top_line(&view), adjustment.value());
        window.destroy();
        assert!(
            shown,
            "a click at line {clicked} during Ctrl+End's travel must keep that line on \
             screen, but the travel ran on to top line {top} (value {value:.0}, clicked \
             at {at_click:.0})"
        );
    }

    /// **The issue's own scenario: a click during Ctrl+End on a document still being
    /// laid out (TDD 9.33).** The travel here is GTK's scroll animating
    /// while validation runs, then the deferred re-issue. MEASURED failing on Windows
    /// with the watch removed (clicked line 11, the view parked at top line 46). Whether
    /// GTK's own animation is still moving when the click lands depends on how fast the
    /// platform validates, so the deterministic guard is the test above; this one says
    /// SKIPPED when the travel never showed a step to click into. With only the `jump`
    /// removed it still passes: the deferred scroll to the caret brings the line back
    /// once the layout is valid, which is the other half of the repair.
    #[gtktest::test]
    fn a_click_during_ctrl_end_on_a_document_still_being_laid_out_keeps_the_caret_on_screen() {
        let _motion = crate::testmotion::AnimatedScrolls::pin();
        let (_buffer, view, scroller, window) = super::super::gtk_integration_tests::cold_editor();
        let adjustment = scroller.vadjustment();
        let view: gtk::TextView = view.upcast();
        let start = adjustment.value();

        view.emit_move_cursor(gtk::MovementStep::BufferEnds, 1, false);
        pump_until_moved(&adjustment, start);
        let at_click = adjustment.value();
        if at_click == start {
            window.destroy();
            println!(
                "SKIPPED [9.33]: Ctrl+End's travel showed no movement within \
                 {FIRST_STEP_DEADLINE:?}, so there was nothing in flight to click into"
            );
            return;
        }
        let clicked = click_top_line(&view);
        assert!(
            caret_shown(&view),
            "precondition: the clicked line is on screen"
        );

        super::super::gtk_integration_tests::pump_until_layout_validated(&adjustment);
        drain_for(Clock::Frame, AFTER_CLICK);
        let (shown, top, value) = (caret_shown(&view), top_line(&view), adjustment.value());
        window.destroy();
        assert!(
            shown,
            "a click at line {clicked} during Ctrl+End's travel must keep that line on \
             screen once the layout settles, but the view shows top line {top} (value \
             {value:.0}, clicked at {at_click:.0})"
        );
    }
}
