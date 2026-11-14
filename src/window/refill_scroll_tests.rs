//! Two reading-position failures that share one mechanism: the preview is rebuilt (its
//! buffer emptied and refilled in place, or its whole widget replaced) while a scroll is
//! still travelling toward the reader's place, and a second writer then lands over it.
//!
//! * **TDD 2.26h, a toggle at the document's end.** The full re-render a refused
//!   disclosure splice falls back to deletes the whole buffer, which drags every mark —
//!   including the `scrib-outline-target` mark an outline jump parked its scroll on — to
//!   offset 0. The restore after the refill must own that parked scroll, and carry the
//!   view the whole way despite unvalidated heights.
//! * **TDD 7.15, a reload during a narrowing drag.** The reload's progressive restore is
//!   still travelling when a narrowing allocation's re-anchor starts a second writer,
//!   aimed at whatever line the transiently clamped viewport happens to read.
//!
//! What is asserted is what the reader sees, except where a case says it asserts a state.
//! The 7.15 case reproduces and is mutation-checked. **The 2.26h cases do not reproduce a
//! failure on the headless stack** (GTK 4.6.9, Xvfb: the warm line restore this replaced
//! also landed, at every jump timing and document size tried), so the end-to-end case is a
//! guard on the property, not a discriminator, and says so; the eviction itself is pinned
//! as a state.

use super::*;
use crate::testpump::{drain_for, until, until_stable, Clock};
use std::time::Duration;

/// Sections in the fixture: enough that the layout is still validating when the event
/// under test lands.
const SECTIONS: usize = 1500;

/// A document of [`SECTIONS`] headed sections ending in a one-line disclosure whose body
/// is literal text inside its opening raw-HTML block, which the fold splice refuses — so
/// toggling it takes the full re-render.
fn document() -> String {
    let mut s = String::from("# Long doc\n\n");
    for n in 1..=SECTIONS {
        s.push_str(&format!(
            "## Section {n:04}\n\nParagraph for section {n}, long enough to wrap onto a second \
             display row in a pane of ordinary width, so that line heights differ from a bare \
             line count. Paragraph for section {n}, long enough to wrap onto a second display \
             row.\n\n"
        ));
    }
    s.push_str(
        "## The end\n\n<details><summary>Compact tail</summary>Compact tail body.</details>\n",
    );
    s
}

fn open(suffix: &str, doc: &str) -> ApplicationWindow {
    let app = crate::window::testkit::test_app_suffixed(suffix);
    let window = crate::window::new_window(&app, "IT", doc, None);
    window.set_default_size(1000, 700);
    window.present();
    until(Clock::Idle, "the window to map", || window.is_mapped());
    window.change_action_state("view-mode", &"preview".to_variant());
    drain_for(Clock::Frame, Duration::from_millis(300));
    window
}

fn preview_view(window: &ApplicationWindow) -> crate::codeview::CodePreviewView {
    state(window)
        .expect("tab")
        .split
        .preview_scroller()
        .and_then(|sw| sw.child())
        .and_then(|c| c.downcast::<crate::codeview::CodePreviewView>().ok())
        .expect("a preview view")
}

fn top_line(window: &ApplicationWindow) -> i32 {
    crate::saferizer::viewport::ViewportTopIter::of(&preview_view(window)).line()
}

/// The preview's adjustment value and its maximum, once both have stopped moving.
fn settled_value(window: &ApplicationWindow) -> (f64, f64) {
    let s = until_stable(
        Clock::Frame,
        Duration::from_secs(20),
        Duration::from_millis(1000),
        || {
            let vadj = preview_view(window)
                .vadjustment()
                .expect("the view is in a scroller");
            (
                vadj.value().round() as i64,
                (vadj.upper() - vadj.page_size()).round() as i64,
            )
        },
    );
    assert!(s.converged, "the preview never settled");
    let (value, max) = s.value.expect("sampled at least once");
    (value as f64, max as f64)
}

/// **TDD 2.26h, as the reader sees it.** A reader who jumped to the end through the
/// outline and then toggles the disclosure there is still at the end afterwards.
///
/// A guard on the property, not a discriminator: it also holds with the warm line restore
/// (`PreviewState::Warm`), which is what 2.26h's re-render used before — see the module
/// doc. The eviction that the refilled restore adds is pinned by
/// [`a_refilled_restore_takes_over_the_outline_jumps_parked_scroll`].
#[gtktest::test]
fn a_toggle_after_an_outline_jump_to_the_end_keeps_the_reader_at_the_end() {
    let doc = document();
    let window = open("refill_scroll.toggle_at_end", &doc);
    let st = state(&window).expect("tab");

    // The outline's own route to the last heading.
    crate::window::panescroll::preview_to_heading(&st, SECTIONS + 1);
    let (before, max) = settled_value(&window);
    assert!(
        max > 10_000.0 && before >= max - 2.0,
        "precondition: the jump reached the end (value {before} of {max})"
    );
    let line_before = top_line(&window);
    let generation_before = crate::farscroll::render_generation(&preview_view(&window).buffer());

    // What a refused splice falls back to.
    rerender_preview_in_place(&window, ViewMode::Preview, RenderShape::ChangedContent);
    // The rebuild retires every navigation still aimed at the discarded render
    // (`farscroll::scroll_to_mark_when_ready`'s re-issue reads this).
    // Mutation-checked: dropping `note_buffer_rebuilt` from `reset_buffer_for_render`.
    assert!(
        crate::farscroll::render_generation(&preview_view(&window).buffer()) > generation_before,
        "the in-place re-render did not advance the buffer's render generation"
    );
    let (after, max_after) = settled_value(&window);
    let line_after = top_line(&window);
    // A line-anchored restore lands the reader's top line at the top edge, which can be
    // a partial line short of the very end.
    assert!(
        after >= max_after - 400.0 && (line_after - line_before).abs() <= 1,
        "the toggle's re-render moved the reader from line {line_before} to {line_after} \
         (value {after} of {max_after})"
    );
    window.destroy();
}

/// **TDD 2.26h, the eviction, as a state.** GTK parks a scroll it cannot honour yet on the
/// mark it was aimed at, and the whole-buffer delete of an in-place re-render moves that
/// mark to offset 0. The restore that follows the refill must move the mark to the
/// reader's line and scroll to it once, so that no request still pending in GTK aims at
/// the top of the document.
///
/// Mutation-checked: removing the move-and-scroll of `scrib-outline-target` from
/// `restore_refilled_preview_to_line` leaves the mark at offset 0.
#[gtktest::test]
fn a_refilled_restore_takes_over_the_outline_jumps_parked_scroll() {
    let doc = document();
    let window = open("refill_scroll.evict", &doc);
    let st = state(&window).expect("tab");

    // An outline jump that has not landed yet: its scroll is parked on the mark.
    crate::window::panescroll::preview_to_heading(&st, SECTIONS + 1);
    let view = preview_view(&window);
    let buffer = view.buffer();
    let mark = buffer
        .mark(crate::codeview::OUTLINE_TARGET_MARK)
        .expect("the jump left its target mark");

    // The rebuild empties the buffer, which is what drags the mark to the start.
    crate::farscroll::note_buffer_rebuilt(&buffer);
    buffer.delete(&mut buffer.start_iter(), &mut buffer.end_iter());
    buffer.insert(&mut buffer.start_iter(), &doc);
    assert_eq!(
        buffer.iter_at_mark(&mark).offset(),
        0,
        "precondition: the delete dragged the mark to the top"
    );

    let sw = st.split.preview_scroller().expect("a preview scroller");
    let reader_line = 900;
    #[expect(
        clippy::disallowed_methods,
        reason = "a test fixture driving the restore seam itself, not a navigation"
    )]
    crate::preview::restore_refilled_preview_to_line(&sw, reader_line);
    assert_eq!(
        buffer.iter_at_mark(&mark).line(),
        reader_line,
        "the restore left the parked jump's mark where the delete dragged it"
    );
    assert_eq!(
        crate::preview::far_restore_in_flight(&view),
        Some(reader_line),
        "the restore did not start its progressive driver"
    );
    window.destroy();
}

/// **TDD 7.15, last bullet.** An external reload that lands in the middle of a narrowing
/// drag re-anchors to the reader's line, not to a near-top position.
///
/// Mutation-checked: dropping `adopt_programmatic_target` from the progressive restore
/// leaves the reader near the top (the resize's re-anchor then reads the transiently
/// clamped viewport). Dropping the join in `reanchor_to_reading_line` alone does not fail
/// this case: the adopted target already makes the resize's own re-anchor aim correctly
/// here, so the two are redundant in it, and the join is the one that survives a
/// re-anchor landing while the restore is still travelling.
#[gtktest::test]
fn a_reload_landing_in_a_narrowing_drag_holds_the_reading_line() {
    let doc = document();
    let window = open("refill_scroll.reload_in_drag", &doc);
    let st = state(&window).expect("tab");

    // Park the reader deep in the document.
    crate::window::panescroll::preview_to_heading(&st, SECTIONS / 2);
    let (before, _) = settled_value(&window);
    let line_before = top_line(&window);
    assert!(
        line_before > 1_000,
        "precondition: the reader is far down (line {line_before}, value {before})"
    );

    // A narrowing drag, one frame-edge step at a time, with the file changing under the
    // reader half way through it.
    let widths = [940, 880, 820, 760, 700, 640, 580, 520, 460, 400];
    for (step, width) in widths.into_iter().enumerate() {
        window.set_default_size(width, 700);
        drain_for(Clock::Frame, Duration::from_millis(60));
        if step == 5 {
            crate::window::reload::apply_external_reload(&window, &format!("{doc}\nappended\n"));
        }
    }
    drain_for(Clock::Frame, Duration::from_millis(300));
    settled_value(&window);
    let narrowed = preview_view(&window).width();
    assert!(
        narrowed < 500,
        "precondition: the drag narrowed the preview (width {narrowed})"
    );

    // A buffer line is a logical line, so the wrap does not move it; what the defect does
    // is land a thousand lines away.
    let after = top_line(&window);
    assert!(
        (after - line_before).abs() < 30,
        "the reader was at line {line_before} and the reload in the drag left them at {after}"
    );
    window.destroy();
}
