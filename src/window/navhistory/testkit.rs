//! Shared fixtures for this module's GTK integration tests.
//!
//! Carries the tests' own cfg rather than a bare `#[cfg(test)]` (POLICY
//! § Testing): every caller is gated on the `gtk-integration-tests` feature, so
//! gated only on `test` these would be dead code under a plain `cargo test` and
//! the `-D warnings` gate would need an `#[allow]` to stay satisfiable.

use crate::codeview::CodePreviewView;
use crate::winstate::{state, TabId};
use gtk::prelude::*;
use gtk::ApplicationWindow;

/// Kept as a name local to these tests; the implementation is the shared one.
pub(super) fn make_app(name: &str) -> gtk::Application {
    crate::window::testkit::test_app(name)
}

/// Whether `win.<name>` is currently enabled — read through the action map the
/// menu item and the toolbar button read, not through the history, so these
/// tests prove the WIRING and not merely the pure core (which has its own
/// tests in `winstate::navhistory`).
pub(super) fn enabled(window: &ApplicationWindow, name: &str) -> bool {
    window
        .lookup_action(name)
        .expect("the action is registered")
        .is_enabled()
}

/// Add a tab with `md`, switching to it exactly as an interactive open does.
pub(super) fn add_tab(window: &ApplicationWindow, md: &str) -> TabId {
    crate::window::create_tab_in_window(window, md, None, false, false).expect("the tab is created")
}

/// A document whose table of contents links to its own sections, with enough
/// filler that a heading's line and the reader's departure line are far apart
/// (an assertion that passed for both would prove nothing).
pub(super) const TOC_DOC: &str = "\
# Guide

- [One](#section-one)
- [Two](#section-two)

## Section One

alpha
alpha
alpha
alpha
alpha
alpha
alpha
alpha

## Section Two

omega
";

/// Three linked sections, for walks that need more than one stop in each
/// direction. Separate from [`TOC_DOC`] rather than an extension of it: the
/// two-section shape is what every existing body asserts against, and widening a
/// shared fixture to serve a new test silently re-aims the old ones.
pub(super) const LONG_TOC_DOC: &str = "\
# Guide

- [One](#section-one)
- [Two](#section-two)
- [Three](#section-three)

## Section One

alpha
alpha
alpha
alpha

## Section Two

beta
beta
beta
beta

## Section Three

omega
omega
omega
omega
";

/// A document whose linked heading is the **first thing in it**, so the
/// navigation target resolves to buffer offset 0 — the mirror of the departure
/// boundary ScrAP-262 was about, approached from the arrival side.
pub(super) const TOP_TARGET_DOC: &str = "\
# Top Section

alpha
alpha
alpha
alpha
alpha
alpha
alpha
alpha

## Elsewhere

[back up](#top-section)

omega
";

/// The active tab's preview scroller and view.
pub(super) fn preview_of(window: &ApplicationWindow) -> (gtk::ScrolledWindow, CodePreviewView) {
    let st = state(window).expect("the window has a tab");
    let sw = st
        .split
        .preview_scroller()
        .expect("the tab shows a preview in the default view mode");
    let view = sw
        .child()
        .and_then(|c| c.downcast::<CodePreviewView>().ok())
        .expect("the scroller wraps the preview view");
    (sw, view)
}

/// Put the reader at `line` the way a scroll does, so `preview_top_line`
/// reports it — `scroll_to_buffer_offset` records its target as the view's
/// tracked reading anchor synchronously (ScrAP-65), which is what makes this
/// assertable without pumping to full allocation (the ScrAP-78 masking trap).
pub(super) fn park_reader_at(view: &CodePreviewView, line: i32) {
    let offset = view
        .buffer()
        .iter_at_line(line)
        .map(|i| i.offset())
        .expect("the line exists");
    view.scroll_to_buffer_offset(offset);
    assert_eq!(
        view.reading_line(),
        line,
        "precondition: the reader is here"
    );
}

/// The buffer line a heading slug renders at, for comparing against where a
/// traversal actually left the viewport.
pub(super) fn line_of(sw: &gtk::ScrolledWindow, slug: &str) -> i32 {
    crate::preview::preview_heading_line(sw, slug).expect("the heading is in this document")
}
