//! The per-window memory gate (TDD 6.13): opening and closing a window must not
//! retain memory cycle after cycle.
//!
//! Compiled only under `--features memory-gates`, alongside [`super::gtk`]. Drives the
//! real `window::new_window` and the real close path, so a retention in any window-owned
//! object counts, wherever it lives.

use crate::memgate::footprint::{assert_bounded, current};
use crate::testpump::{self, Clock};
use crate::window::testkit::test_app_suffixed;
use gtk::prelude::*;
use std::time::Duration;

/// Cycles discarded before judging. More than the renders' shared warm-up: on Windows the
/// first window cycles keep growing for three or four more after that (about 7 MB in all,
/// then flat), and a second warm-up step lands in the residual, failing two runs in three.
const WINDOW_WARMUP: usize = 8;

/// Cycles sampled: enough that the ~0.4 MB a leaking window kept climbs well past the
/// residual bound, few enough to stay in the step's seconds.
const WINDOW_SAMPLES: usize = WINDOW_WARMUP + 10;

/// Sections in the document each cycle's window shows: sized so that a window which keeps its
/// document alive (the editor's buffer, the rendered preview) keeps tens of kilobytes more
/// than the welcome text would, which a small leak could otherwise hide inside the bound.
const SECTIONS: usize = 20;

/// A document touching every renderer family a window builds (headings, emphasis, a link, a
/// list, a code block), long enough to be a real one.
fn document() -> String {
    (0..SECTIONS)
        .map(|i| {
            format!(
                "## Section {i}\n\nSome *emphasis*, `code`, and a [link](https://example.com/{i}).\n\n\
                 - one\n- two\n\n```rust\nfn f{i}() {{}}\n```\n\n"
            )
        })
        .collect()
}

/// One View > New Window and close of a whole window, ending with every reference of ours
/// dropped and the main loop drained, so only a retention the application did not intend
/// remains. The window is the one Ctrl+N opens: built from a source window.
fn cycle(app: &gtk::Application, source: &gtk::ApplicationWindow, text: &str) {
    let window = crate::window::new_window_from_source(app, "gate", text, None, Some(source));
    window.present();
    testpump::until(Clock::Idle, "the window to map", || window.is_mapped());
    testpump::drain_for(Clock::Frame, Duration::from_millis(50));
    window.close();
    drop(window);
    testpump::drain_for(Clock::Frame, Duration::from_millis(50));
}

#[gtktest::test]
fn window_open_close_cycles_do_not_grow_footprint_ttd_6_13() {
    let Some(_measuring) = crate::memgate::footprint::measuring("6.13") else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let app = test_app_suffixed("memgate-window-cycles");
        // The window kept open throughout, as the reader's own is.
        let source = crate::window::new_window(&app, "source", "# source\n", None);
        let text = document();
        let mut samples = Vec::with_capacity(WINDOW_SAMPLES);
        for _ in 0..WINDOW_SAMPLES {
            cycle(&app, &source, &text);
            samples.push(current().expect("footprint"));
        }
        assert_bounded("6.13 window open/close", WINDOW_WARMUP, &samples);
    });
}

/// Every widget under `root`, weakly, `root` included.
fn weak_tree(root: &gtk::Widget, out: &mut Vec<gtk::glib::WeakRef<gtk::Widget>>) {
    out.push(root.downgrade());
    let mut child = root.first_child();
    while let Some(c) = child {
        weak_tree(&c, out);
        child = c.next_sibling();
    }
}

/// Widget types a closed window leaves behind that are the toolkit's, not the application's:
/// GTK's one shared tooltip window. Named so that anything else surviving is a failure, and
/// so this list shrinks rather than grows.
const TOOLKIT_SURVIVORS: [&str; 1] = ["GtkTooltipWindow"];

/// TDD 6.13, the finalization half: a closed window's widgets are gone, so a retention shows
/// by name rather than as bytes in a series. A footprint series cannot see a small
/// retention in a short run and a weak reference cannot miss one.
///
/// Walks the window's whole tree while it is open, closes it, drops every reference this test
/// holds, drains the main loop, and lists what is still alive. The survivors it reports are
/// the roots a leaked subtree hangs from, which is where to start looking.
#[gtktest::test]
fn window_widgets_are_finalized_after_close_ttd_6_13() {
    let dir = tempfile::tempdir().unwrap();
    crate::session::with_state_home_for_test(dir.path(), || {
        let app = test_app_suffixed("memgate-window-finalizes");
        let source = crate::window::new_window(&app, "source", "# source\n", None);
        let text = document();
        // One cycle first: whatever a first window warms once is not a retention.
        cycle(&app, &source, &text);

        let window =
            crate::window::new_window_from_source(&app, "gate", &text, None, Some(&source));
        window.present();
        testpump::until(Clock::Idle, "the window to map", || window.is_mapped());
        testpump::drain_for(Clock::Frame, Duration::from_millis(50));
        let mut tree = Vec::new();
        weak_tree(window.upcast_ref(), &mut tree);
        let buffer = crate::winstate::state(&window)
            .expect("the window has a tab")
            .editor_buf
            .downgrade();
        window.close();
        drop(window);
        testpump::drain_for(Clock::Frame, Duration::from_millis(300));

        let tooltip = gtk::glib::Type::from_name("GtkTooltipWindow");
        let mut survivors: Vec<String> = tree
            .iter()
            .filter_map(|weak| weak.upgrade())
            // By type first, and before anything reads the widget's place in a tree: the
            // renderer in the list below outlives the gutter that was its parent, and its
            // parent pointer dangles (reading it crashes).
            .filter(|widget| !TOOLKIT_SURVIVORS.contains(&widget.type_().name()))
            // The tooltip window's own box, icon and label are its, as it is the toolkit's.
            .filter(|widget| tooltip.is_none_or(|t| widget.ancestor(t).is_none()))
            .map(|widget| widget.type_().name().to_string())
            .collect();
        survivors.sort();
        assert!(
            survivors.is_empty(),
            "TDD 6.13: {} of {} widgets of a closed window are still alive: {survivors:?}",
            survivors.len(),
            tree.len()
        );
        assert!(
            buffer.upgrade().is_none(),
            "TDD 6.13: the closed window's editor buffer, and with it the document's text, is still alive"
        );
    });
}
