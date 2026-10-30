//! The preview's one-selection rule for table cells, and the action card's surface.
//!
//! In its own file because `overlay.rs` is past the soft line limit.

use super::{action_card_of, claim_selection_for_press};
use crate::codeview::CodePreviewView;
use crate::testpump::{destroy_after_tooltip_delay, drain_for, until, until_or_for, Clock};
use gtk::prelude::*;
use std::time::Duration;

const MD: &str = "Body prose to select before the table.\n\n\
    | Name | Value |\n|------|-------|\n| alpha cell | beta cell |\n| gamma cell | delta cell |\n";

/// Render [`MD`] into a presented window and return it with its view and its cells.
fn table_window() -> (gtk::Window, CodePreviewView, Vec<gtk::Label>) {
    let win = gtk::Window::new();
    let pane = crate::preview::render(
        None,
        MD,
        None,
        1.0,
        false,
        &crate::fold::FoldState::default(),
    );
    let view = pane
        .downcast_ref::<gtk::Overlay>()
        .expect("preview pane is a GtkOverlay")
        .child()
        .and_downcast::<gtk::ScrolledWindow>()
        .expect("overlay wraps the scroller")
        .child()
        .and_downcast::<CodePreviewView>()
        .expect("scroller holds the CodePreviewView");
    // The action card only shows with an annotation sink (the split view wires one).
    view.set_annotation_sink(std::rc::Rc::new(|_| {}));
    win.set_default_size(700, 400);
    win.set_child(Some(&pane));
    win.present();
    until(Clock::Frame, "the table's cells to lay out", || {
        crate::preview::scrib_labels(&view).is_some_and(|l| {
            let l = l.borrow();
            l.len() >= 4 && l.iter().all(|c| c.width() > 0)
        })
    });
    let cells = crate::preview::scrib_labels(&view)
        .expect("the table registered its cells")
        .borrow()
        .clone();
    (win, view, cells)
}

/// The cell whose text is `text`.
fn cell(cells: &[gtk::Label], text: &str) -> gtk::Label {
    cells
        .iter()
        .find(|l| l.text() == text)
        .cloned()
        .unwrap_or_else(|| panic!("no cell reads {text:?}"))
}

/// The centre of `label`, in `view`'s coordinates.
fn centre_of(label: &gtk::Label, view: &CodePreviewView) -> (f64, f64) {
    label
        .translate_coordinates(
            view,
            f64::from(label.width()) / 2.0,
            f64::from(label.height()) / 2.0,
        )
        .expect("the cell is inside the view")
}

/// **A press in another cell clears the first cell's selection.**
///
/// Each cell is its own selectable label, and a label's selection is sticky: before the
/// fix, clicking a different cell left the first cell highlighted, and the Annotate card
/// its selection raised stayed up over it (TDD 17.63). Mutation: drop
/// the clearing loop from `claim_selection_for_press` and this fails.
#[gtktest::test]
fn a_press_in_another_cell_clears_the_first_cells_selection() {
    let (win, view, cells) = table_window();
    let alpha = cell(&cells, "alpha cell");
    let delta = cell(&cells, "delta cell");
    alpha.select_region(0, 5);
    assert!(
        alpha.selection_bounds().is_some(),
        "precondition: the first cell holds a selection"
    );
    let (x, y) = centre_of(&delta, &view);
    claim_selection_for_press(&view, x, y);
    assert!(
        alpha.selection_bounds().is_none(),
        "a press in another cell must clear the first cell's selection"
    );
    destroy_after_tooltip_delay(&win, || ());
}

/// **A press in a cell clears the body text's selection.** A cell press claims no
/// selection of its own, so without this the body's selection — and the Annotate card
/// over it — stayed. Mutation: drop the body collapse and this fails.
#[gtktest::test]
fn a_press_in_a_cell_clears_the_body_selection() {
    let (win, view, cells) = table_window();
    let delta = cell(&cells, "delta cell");
    let buf = view.buffer();
    buf.select_range(&buf.iter_at_offset(0), &buf.iter_at_offset(4));
    assert!(
        buf.has_selection(),
        "precondition: the body holds a selection"
    );
    let (x, y) = centre_of(&delta, &view);
    claim_selection_for_press(&view, x, y);
    assert!(
        !buf.has_selection(),
        "a press in a cell must clear the body text's selection"
    );
    destroy_after_tooltip_delay(&win, || ());
}

/// **A press in body text or empty page space clears a cell's selection, and leaves the
/// body's own selection to GTK.** The cell half used to ride on the text view moving its
/// cursor, which a click GTK swallows (inside the double-click time, see
/// `wire_cell_press_exclusivity`) never does. The body half must stay GTK's, or a
/// Shift-press extending a body selection would be collapsed before GTK saw it.
/// Mutation: return early for a body press before the clearing loop and the first
/// assertion fails; collapse the body there and the second does.
#[gtktest::test]
fn a_press_in_body_text_clears_the_cells_and_leaves_the_body_to_gtk() {
    let (win, view, cells) = table_window();
    let alpha = cell(&cells, "alpha cell");
    let buf = view.buffer();
    let rect = view.iter_location(&buf.iter_at_offset(2));
    let (x, y) = view.buffer_to_window_coords(gtk::TextWindowType::Widget, rect.x(), rect.y());
    let (x, y) = (f64::from(x), f64::from(y + 2));

    alpha.select_region(0, 5);
    assert!(
        alpha.selection_bounds().is_some(),
        "precondition: the cell holds a selection"
    );
    claim_selection_for_press(&view, x, y);
    assert!(
        alpha.selection_bounds().is_none(),
        "a press in body text must clear a cell's selection"
    );

    buf.select_range(&buf.iter_at_offset(0), &buf.iter_at_offset(4));
    claim_selection_for_press(&view, x, y);
    assert!(
        buf.has_selection(),
        "a body press leaves the body selection to GTK"
    );
    destroy_after_tooltip_delay(&win, || ());
}

/// **A press away from a focused cell takes focus off it, without selecting any cell.**
///
/// A quick press GTK denies to every click gesture moved focus nowhere, so the old cell
/// kept a blinking text cursor after its selection was cleared (TDD 17.63). Calling the
/// press handler alone is that press: no gesture runs. Both destinations are checked —
/// another cell and body text. Mutation: drop the `release_cell_focus` call and the
/// focus assertions fail.
///
/// Only an active window gives a widget focus, so the test skips when another
/// application holds the front (seen on macOS while the suite ran in the background):
/// the precondition could never hold. A reader's press activates the window before
/// the press handler runs, so the product has no such gap. Which inactive windows may
/// skip is [`inactive_window_skip`]'s decision; every other one is a failure.
#[gtktest::test]
fn a_press_away_from_a_focused_cell_takes_its_focus() {
    let before = crate::platform::frontmost_pid();
    let (win, view, cells) = table_window();
    if !until_or_for(Clock::Frame, Duration::from_secs(2), || win.is_active()) {
        let after = crate::platform::frontmost_pid();
        let me = i32::try_from(std::process::id()).expect("a pid fits in pid_t");
        // Named, not just counted: under tmux, screen or ssh the launching terminal is no
        // ancestor of this process, so a reader must be able to see WHICH application a
        // skip blamed.
        let front = crate::platform::frontmost_bundle_id().unwrap_or_else(|| "?".to_owned());
        let Some(why) =
            inactive_window_skip(before, after, me, crate::platform::is_ancestor_process)
        else {
            panic!(
                "the test window never became the active window (front before present: \
                 {before:?}, after the wait: {after:?} ({front}), this process: {me})"
            );
        };
        crate::testsymlink::skipped(
            "TDD 17.63",
            &format!(
                "the test window never became the active window because {why} ({front}), \
                 so no cell can take focus"
            ),
        );
        destroy_after_tooltip_delay(&win, || ());
        return;
    }
    let alpha = cell(&cells, "alpha cell");
    let delta = cell(&cells, "delta cell");
    let buf = view.buffer();
    let rect = view.iter_location(&buf.iter_at_offset(2));
    let (bx, by) = view.buffer_to_window_coords(gtk::TextWindowType::Widget, rect.x(), rect.y());
    let body = (f64::from(bx), f64::from(by + 2));
    let elsewhere = [
        ("another cell", centre_of(&delta, &view)),
        ("body text", body),
    ];

    for (what, (x, y)) in elsewhere {
        alpha.grab_focus();
        alpha.select_region(0, 5);
        until(Clock::Frame, "the first cell to hold focus", || {
            alpha.has_focus()
        });
        claim_selection_for_press(&view, x, y);
        assert!(
            !alpha.has_focus(),
            "a press in {what} must take focus off the first cell"
        );
        assert!(
            cells.iter().all(|c| c.selection_bounds().is_none()),
            "moving focus off the cell must not select any cell's text ({what})"
        );
    }
    destroy_after_tooltip_delay(&win, || ());
}

/// Why a test window that never became active may be skipped rather than failed, from
/// the frontmost application sampled before it was presented and after the wait, or
/// `None` when it must fail.
///
/// "Another application is in front" is ALSO what a failed activation leaves: the
/// application that held the front before simply keeps it. So that alone is not enough.
/// It skips when the front went to some other process during the wait (the developer
/// switched away), or when an application that did NOT launch this process held the
/// front throughout (the suite running behind other work, where macOS declines to hand
/// the front over). It fails when this process holds the front with its window still
/// inactive, when nothing answers (every platform but macOS), and when the application
/// that kept the front is the one that launched the suite: taking the front from the
/// terminal it was started from is exactly what an activating window does.
fn inactive_window_skip(
    before: Option<i32>,
    after: Option<i32>,
    me: i32,
    launched_us: impl Fn(i32) -> bool,
) -> Option<&'static str> {
    let after = after.filter(|&pid| pid != me)?;
    match before {
        Some(before) if before == after && launched_us(before) => None,
        Some(before) if before == after => {
            Some("an application that did not launch it held the front throughout")
        }
        _ => Some("another application took the front during the wait"),
    }
}

/// Mutation: drop the `launched_us` arm and the terminal case skips; drop the
/// `before == after` arms and a failed activation behind an unrelated application reads
/// as the developer switching away (still a skip, but for the wrong stated reason).
#[gtktest::test]
fn only_an_application_that_kept_or_took_the_front_excuses_an_inactive_window() {
    const ME: i32 = 100;
    const TERMINAL: i32 = 50;
    const OTHER: i32 = 70;
    let launched = |pid| pid == TERMINAL;
    let skip = |before, after| inactive_window_skip(before, after, ME, launched);
    // Fails: nothing answers, this process is in front, or the launcher never let go.
    assert_eq!(skip(None, None), None);
    assert_eq!(skip(Some(TERMINAL), Some(ME)), None);
    assert_eq!(skip(Some(TERMINAL), Some(TERMINAL)), None);
    // Skips: an unrelated application kept the front, or another one took it.
    assert!(skip(Some(OTHER), Some(OTHER)).is_some_and(|w| w.contains("throughout")));
    assert!(skip(Some(TERMINAL), Some(OTHER)).is_some_and(|w| w.contains("took")));
    assert!(skip(Some(ME), Some(OTHER)).is_some_and(|w| w.contains("took")));
    assert!(skip(None, Some(OTHER)).is_some_and(|w| w.contains("took")));
}

/// The launcher arm above is only as good as the ancestry it asks about: an
/// `is_ancestor_process` that always answered `false` would turn every terminal case
/// into a skip. macOS only, the one platform that answers.
#[cfg(target_os = "macos")]
#[gtktest::test]
fn the_process_that_launched_the_suite_reads_as_its_ancestor() {
    let parent = i32::try_from(std::os::unix::process::parent_id()).expect("a pid fits");
    let me = i32::try_from(std::process::id()).expect("a pid fits");
    assert!(crate::platform::is_ancestor_process(parent), "the parent");
    assert!(
        crate::platform::is_ancestor_process(1),
        "launchd, every process's root"
    );
    assert!(!crate::platform::is_ancestor_process(me), "not itself");
}

/// **The cell-press rule is delivered by a capture-phase legacy controller on the view
/// (TDD 17.63, GTK4Rs/AP-363).** The tests above call the press handler directly,
/// because gtk-rs cannot build the raw `gdk::Event` the controller is handed; its event
/// filter and coordinate transform are unit-tested in `annotate::press`. This is the
/// half neither reaches: that the controller is wired at all, and in the phase that
/// sees a press before the cell does. Mutation: drop `wire_cell_press_exclusivity(view)`
/// from `wire_annotation_overlay`, or set its phase to bubble, and this fails.
#[gtktest::test]
fn the_view_carries_the_capture_phase_cell_press_controller() {
    let (win, view, _) = table_window();
    let controllers = view.observe_controllers();
    let capture_legacy = (0..controllers.n_items())
        .filter_map(|i| controllers.item(i))
        .filter_map(|c| c.downcast::<gtk::EventControllerLegacy>().ok())
        .filter(|c| c.propagation_phase() == gtk::PropagationPhase::Capture)
        .count();
    assert_eq!(
        capture_legacy, 1,
        "the preview view carries exactly one capture-phase EventControllerLegacy, the \
         cell-press rule's"
    );
    destroy_after_tooltip_delay(&win, || ());
}

/// **The Annotate action card is part of the window's own surface (TDD 17.62).** A popover is its
/// own window-system surface, which the window manager left on screen when the window was
/// minimized or moved to another desktop. Mutation: build the action card as a popover
/// again and this fails.
#[gtktest::test]
fn the_action_card_lives_in_the_windows_own_surface() {
    let (win, view, _) = table_window();
    let buf = view.buffer();
    buf.select_range(&buf.iter_at_offset(0), &buf.iter_at_offset(4));
    let card = action_card_of(&view).expect("the action card is wired onto the view");
    until(Clock::Frame, "the action card to show", || {
        card.is_visible()
    });
    drain_for(Clock::Frame, Duration::from_millis(100));
    assert_eq!(
        card.native(),
        win.native(),
        "the action card must share the window's native surface"
    );
    destroy_after_tooltip_delay(&win, || card.set_visible(false));
}
