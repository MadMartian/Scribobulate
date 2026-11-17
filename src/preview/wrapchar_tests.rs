//! **The preview wraps prose at spaces and never overflows the pane** (TDD 2.2·a11y),
//! on whichever GTK this runs on.
//!
//! Test-only, and carrying `gtk-integration-tests` for the reason
//! `preview::altsuppression` gives: laying a paragraph out needs a realized view. Both
//! paths are asserted, neither is skipped: below GTK 4.7 the paragraph holding the wide
//! run must carry `wrap-char` and the prose must not; from 4.7 no paragraph carries it
//! and `WordChar` does the work. The layout assertions are the same on both, which is
//! the contract: lines end at spaces, and nothing is wider than the pane.

use crate::codeview::CodePreviewView;
use crate::testpump::{drain_for, until, Clock};
use gtk::prelude::*;
use std::time::Duration;

const PROSE: &str = "The quick brown fox jumps over the lazy dog and keeps on running \
through the long meadow until the evening falls softly over the distant hills again.";

/// How many prose paragraphs the fixture holds, each opening with a word one character
/// longer than the last. Where a line fills is a matter of luck against word lengths, so
/// one paragraph can break at a space under character wrapping by chance; staggering the
/// opening word moves every later break, and no staggering lets them all land on spaces.
const PROSE_PARAGRAPHS: usize = 8;

/// A fixture in a pane at the window's width floor: [`PROSE_PARAGRAPHS`] prose
/// paragraphs, then a 200-character run with no break opportunity in it.
fn narrow_preview() -> (gtk::Window, CodePreviewView, String) {
    let wide = "a".repeat(200);
    let mut md = String::new();
    for k in 1..=PROSE_PARAGRAPHS {
        md.push_str(&format!("{} {PROSE}\n\n", "w".repeat(k)));
    }
    md.push_str(&format!("{wide}\n"));
    let pane = crate::preview::render(
        None,
        &md,
        None,
        1.0,
        false,
        &crate::fold::FoldState::default(),
    );
    let view = crate::preview::view_of(&pane).expect("the preview tree render() built");
    let window = gtk::Window::new();
    window.set_default_size(crate::window::MIN_WINDOW_WIDTH, 500);
    window.set_child(Some(&pane));
    window.present();
    until(Clock::Frame, "the preview maps", || view.is_mapped());
    drain_for(Clock::Frame, Duration::from_millis(400));
    (window, view, wide)
}

/// The start offsets of every display line of the paragraph that starts at `start`.
fn display_line_starts(view: &CodePreviewView, start: i32) -> Vec<i32> {
    let buf = view.buffer();
    let mut it = buf.iter_at_offset(start);
    let mut para_end = it;
    para_end.forward_to_line_end();
    let mut starts = vec![start];
    while view.forward_display_line(&mut it) && it.offset() < para_end.offset() {
        starts.push(it.offset());
    }
    starts
}

#[gtktest::test]
fn prose_breaks_at_spaces_and_a_wide_run_stays_inside_the_pane() {
    let (_window, view, wide) = narrow_preview();
    let buf = view.buffer();
    let policy = crate::tags::wrap_policy();

    // Which path this GTK takes, asserted rather than assumed.
    assert_eq!(view.wrap_mode(), policy.mode);
    let wrap_char = buf
        .tag_table()
        .lookup(crate::tags::TagName::WrapChar.name())
        .expect("the wrap-char tag is registered on every GTK");
    let text = buf.slice(&buf.start_iter(), &buf.end_iter(), true);
    let wide_start = text
        .find(&wide)
        .map(|byte| text[..byte].chars().count() as i32)
        .expect("the wide run is in the buffer");
    let has_tag = |offset: i32| buf.iter_at_offset(offset).has_tag(&wrap_char);
    assert_eq!(
        has_tag(wide_start),
        policy.char_for_wide_runs,
        "below GTK 4.7 the paragraph holding the 200-character run is switched to \
         character wrapping, and from 4.7 nothing is (policy {policy:?})"
    );

    // Prose: no paragraph is switched, and every display line but the last ends at a
    // space.
    let mut prose_lines = 0;
    for line in 0..buf.line_count() {
        let start = buf
            .iter_at_line(line)
            .expect("a line of the buffer")
            .offset();
        if start >= wide_start || buf.iter_at_offset(start).char() != 'w' {
            continue;
        }
        assert!(
            !has_tag(start),
            "a paragraph with no run too wide to fit must keep word wrapping (line {line})"
        );
        let starts = display_line_starts(&view, start);
        assert!(
            starts.len() >= 3,
            "precondition: the prose must wrap at a {}px pane, got display lines at {starts:?}",
            crate::window::MIN_WINDOW_WIDTH
        );
        for &s in &starts[1..] {
            let before = buf.iter_at_offset(s - 1).char();
            assert_eq!(
                before,
                ' ',
                "a display line broke mid-word: it ends with {before:?} before offset {s}, in \
                 {:?}",
                buf.slice(
                    &buf.iter_at_offset(s - 12),
                    &buf.iter_at_offset(s + 6),
                    false
                )
            );
        }
        prose_lines += 1;
    }
    assert_eq!(
        prose_lines, PROSE_PARAGRAPHS,
        "every prose paragraph was checked"
    );

    // The wide run: wraps, and no character lies past the pane's edge.
    let lines = display_line_starts(&view, wide_start);
    assert!(
        lines.len() >= 2,
        "precondition: a 200-character run cannot fit one line of a {}px pane",
        crate::window::MIN_WINDOW_WIDTH
    );
    let pane_w = view.width();
    assert!(pane_w > 0, "the view has no width: nothing was laid out");
    for i in 0..wide.chars().count() as i32 {
        let r = view.iter_location(&buf.iter_at_offset(wide_start + i));
        assert!(
            r.x() + r.width() <= pane_w,
            "character {i} of the wide run ends at x={} past the {pane_w}px pane",
            r.x() + r.width()
        );
    }
}
