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
    settle_footprint();
}

/// Longest this waits for one cycle's footprint to go quiet.
const SETTLE_CAP: Duration = Duration::from_secs(5);
/// How long the footprint must hold still (within [`SETTLE_STEP`]) to count as settled.
const SETTLE_QUIET: Duration = Duration::from_millis(1200);
/// A change smaller than this is noise, not the window server's late release.
const SETTLE_STEP: u64 = 512 * 1024;

/// Wait until the process footprint has held still, so the sample that follows is a
/// reading of what the cycle RETAINED and not of what the OS has not yet given back.
///
/// MEASURED, macOS seat, unlocked, release build: a closed window's backing surface (~10.3 MB
/// for the 900x720 default at 2x) is released by the window server up to about a second
/// AFTER the close, at a different cycle each run. The series is then flat apart from ±10.3 MB
/// steps (a rise at one cycle, its fall a few later), and the per-stretch rule judges any
/// stretch that holds two rises and no fall as a ~10 MB climb: 4 of 8 runs failed on a build
/// that retains nothing (the CI runner's 4.8 MB steps are the same shape at its own surface
/// size). With 400 ms between cycles the noise stayed; with 800 ms and 1000 ms, 10 runs in 10
/// were flat. The finalization half passes throughout: the application released everything.
///
/// A leak is untouched by this: a cycle that keeps 0.55 MB keeps it however long one waits.
fn settle_footprint() {
    let start = std::time::Instant::now();
    let mut reference = current().expect("footprint");
    let mut quiet_since = start;
    while start.elapsed() < SETTLE_CAP && quiet_since.elapsed() < SETTLE_QUIET {
        testpump::drain_for(Clock::Frame, Duration::from_millis(100));
        let now = current().expect("footprint");
        if now.abs_diff(reference) > SETTLE_STEP {
            reference = now;
            quiet_since = std::time::Instant::now();
        }
    }
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

/// TDD 6.13: two live windows' menu models link the SAME File, Edit and Help submenu
/// (and the same styles-and-blocks section of Format). GTK 4.6.9's menu bar keeps every
/// submenu model it was given after its window closes (GTK4Rs/AP-387), which no widget
/// reference shows, so what keeps a closed window's models from piling up is that they
/// were never the window's own. Mutation: build `menubar::SharedMenus` per window and
/// this fails, while the finalization half below stays green.
fn assert_menus_shared(a: &gtk::ApplicationWindow, b: &gtk::ApplicationWindow) {
    let submenu = |window: &gtk::ApplicationWindow, index: i32| {
        crate::winstate::chrome(window)
            .expect("the window has chrome")
            .menu_model
            .item_link(index, "submenu")
            .expect("a top-level submenu")
    };
    for (index, title) in [(0, "File"), (1, "Edit"), (4, "Help")] {
        assert_eq!(
            submenu(a, index),
            submenu(b, index),
            "TDD 6.13: the {title} menu is built per window, so a closed window's copy is kept for good"
        );
    }
}

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
        assert_menus_shared(&source, &window);
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
