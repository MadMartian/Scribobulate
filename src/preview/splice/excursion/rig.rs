//! The rig the [`super`] measurement runs on: a presented preview pane over a tall
//! document, plus the settle discipline every reading it takes depends on.
//!
//! Split out of `excursion.rs` at the 500-line soft limit (POLICY § Code style). The
//! cut is by cause rather than by size: this file owns **establishing a state you may
//! legitimately read geometry from**, and its sibling owns **the experiment run against
//! that state**. Everything here is precondition; nothing here is a measurement.

use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::codeview::CodePreviewView;
use crate::fold::FoldState;
use crate::preview::build::{
    apply_preview_margins, attach_anchored, build_render_products_with_theme, install_content,
    RenderProducts,
};
use crate::testpump::{self, Clock};

use super::harness::{PANE_H, PANE_W, QUIET, READING_FRACTION, SETTLE_DEADLINE, ZOOM};

/// One reading of the vadjustment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Reading {
    pub(super) value: f64,
    pub(super) upper: f64,
    pub(super) page_size: f64,
}

impl Reading {
    pub(super) fn of(adjustment: &gtk::Adjustment) -> Self {
        Reading {
            value: adjustment.value(),
            upper: adjustment.upper(),
            page_size: adjustment.page_size(),
        }
    }
}

impl std::fmt::Display for Reading {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "value {:>9.0}  upper {:>9.0}  page {:>6.0}",
            self.value, self.upper, self.page_size
        )
    }
}

/// A presented preview pane, with everything a splice needs held by the caller.
///
/// Built by hand rather than through [`crate::preview::render`] because the splice
/// takes the PRE-splice render's own `anchored` list and `disclosure_extents`, and
/// nothing exposes those for reading off a live view — which is one of the three
/// things the plan records as blocking the wiring. Everything the view is handed here
/// is handed to it by the same functions `render` uses, in the same order.
pub(super) struct Rig {
    pub(super) view: CodePreviewView,
    pub(super) scroller: gtk::ScrolledWindow,
    pub(super) window: gtk::Window,
    pub(super) buf: gtk::TextBuffer,
    pub(super) anchored: Vec<(gtk::TextChildAnchor, gtk::Widget)>,
    pub(super) extents: Vec<crate::renderer::DisclosureExtent>,
}

impl Rig {
    /// Build the pane, present it, and settle it — with this view's `top-margin`
    /// optionally overridden after [`apply_preview_margins`] has applied the
    /// configured one.
    ///
    /// **The override is on THIS view and nothing else.** `top-margin` is a per-view
    /// widget property, so an experiment that varies it varies it here — never in
    /// `config.rs`, whose value is what the application ships with and what
    /// `preview::interactions` asserts about. `None` leaves the configured value
    /// standing, which is what every experiment but [`super::margin`] wants.
    ///
    /// The set is asserted to have taken. A margin knob that silently did not apply
    /// would make every reading below answer for the configured margin while being
    /// reported against another one (ScrAP-252's family) — and the failure would look
    /// exactly like the null result the experiment is testing for.
    pub(super) fn new(md: &str, folds: &FoldState, top_margin: Option<i32>) -> Self {
        let RenderProducts {
            buf,
            anchored,
            disclosure_extents,
            install,
            ..
        } = build_render_products_with_theme(md, None, ZOOM, false, crate::theme::active(), folds);

        let view = CodePreviewView::new();
        view.add_css_class("scrib-preview");
        // Before the view is realized, so this is not the ScrAP-104 buffer swap.
        view.set_buffer(Some(&buf));
        view.set_editable(false);
        view.set_wrap_mode(gtk::WrapMode::Char);
        view.set_cursor_visible(false);
        apply_preview_margins(&view, ZOOM);
        if let Some(px) = top_margin {
            view.set_top_margin(px);
            assert_eq!(
                view.top_margin(),
                px,
                "the top-margin knob did not take: asked for {px}px and the view \
                 reports {}px. Every reading from this rig would describe a margin \
                 other than the one it is reported under.",
                view.top_margin(),
            );
        }
        install_content(&view, install, ZOOM);
        attach_anchored(&view, &anchored);

        let scroller = gtk::ScrolledWindow::new();
        // A DIRECT child, because `re_render` resolves the view as `sw.child()`.
        scroller.set_child(Some(&view));
        let window = gtk::Window::new();
        window.set_default_size(PANE_W, PANE_H);
        window.set_child(Some(&scroller));
        window.present();

        let rig = Rig {
            view,
            scroller,
            window,
            buf,
            anchored,
            extents: disclosure_extents,
        };
        rig.settle();
        rig
    }

    pub(super) fn adjustment(&self) -> gtk::Adjustment {
        self.scroller.vadjustment()
    }

    /// Pump until GTK has finished validating line heights AND the range has stopped
    /// moving.
    ///
    /// Both halves, deliberately. `after_line_heights_validated` is the project's
    /// exact "the layout is valid now" event (GTK4Rs/T-5) and is the primary oracle —
    /// but its own rustdoc records that a main loop pumped from *inside* the validate
    /// callback's stack can dispatch it early, and anchored-child allocation is the
    /// realistic route, which this fixture has (every disclosure toggle is an anchored
    /// child). The quiet window behind it costs ~300 ms and closes that hole; reading
    /// geometry a fraction too early is exactly how this project has repeatedly
    /// measured the wrong thing.
    pub(super) fn settle(&self) {
        let adjustment = self.adjustment();
        {
            let view = self.view.clone();
            let adjustment = adjustment.clone();
            testpump::until(
                Clock::Idle,
                "the preview to map and acquire a viewport",
                move || view.is_mapped() && adjustment.page_size() > 0.0,
            );
        }

        let fired = Rc::new(Cell::new(false));
        {
            let f = Rc::clone(&fired);
            crate::farscroll::after_line_heights_validated(self.view.upcast_ref(), move |_| {
                f.set(true)
            });
        }
        testpump::until_for(
            Clock::Idle,
            SETTLE_DEADLINE,
            "line heights to validate",
            move || fired.get(),
        );

        let settled = testpump::until_stable(Clock::Idle, SETTLE_DEADLINE, QUIET, {
            let adjustment = adjustment.clone();
            move || adjustment.upper().to_bits()
        });
        assert!(
            settled.converged,
            "the vadjustment range never stopped moving, so every reading taken after \
             this measures the machine rather than the code (last upper {:?})",
            settled.value.map(f64::from_bits),
        );
    }

    /// Settle while recording the lowest `value` and `upper` seen on the way — the
    /// excursion itself. See the module docs on why the trough and not the endpoints.
    pub(super) fn settle_watching_the_trough(&self) -> (f64, f64) {
        let adjustment = self.adjustment();
        let mut min_value = f64::INFINITY;
        let mut min_upper = f64::INFINITY;
        let settled = testpump::until_stable(Clock::Idle, SETTLE_DEADLINE, QUIET, || {
            min_value = min_value.min(adjustment.value());
            min_upper = min_upper.min(adjustment.upper());
            adjustment.upper().to_bits()
        });
        assert!(
            settled.converged,
            "the vadjustment range never stopped moving after the toggle"
        );
        (min_value, min_upper)
    }

    /// See [`top_line_text`].
    pub(super) fn top_line_text(&self) -> String {
        top_line_text(&self.view)
    }

    /// See [`anchor_reader`].
    pub(super) fn anchor_reader(&self) -> gtk::TextMark {
        anchor_reader(&self.view)
    }

    /// See [`reader_offset`].
    pub(super) fn reader_offset(&self, mark: &gtk::TextMark) -> (f64, String) {
        reader_offset(&self.view, &self.adjustment(), mark)
    }

    /// Park the reader well down the document.
    pub(super) fn scroll_to_reading_position(&self) {
        let adjustment = self.adjustment();
        let target = (adjustment.upper() - adjustment.page_size()).max(0.0) * READING_FRACTION;
        crate::saferizer::scrollpos::jump(&adjustment, target);
        self.settle();
    }

    pub(super) fn teardown(self) {
        self.window.destroy();
    }
}

// ── Reading the reader's place ──────────────────────────────────────────────────
//
// Free functions rather than [`Rig`] methods, because a SECOND rig needs exactly these
// three and nothing else this file offers: [`super::wired`] drives the whole
// application (a real window and tab, so the toggle's own handler runs) instead of
// building a pane by hand, and a private copy of "where is the reader?" would be free
// to disagree with the one every other measurement in this directory is reported
// against. `Rig`'s methods below delegate here.

/// The text of the line at the top of `view`'s viewport — the reader's actual place,
/// which a buffer OFFSET cannot express across a toggle that inserts text above it.
/// Read through the `saferizer` seam rather than a hand-rolled `line_at_y` (ScrAP-263).
pub(super) fn top_line_text(view: &CodePreviewView) -> String {
    let start = crate::saferizer::viewport::ViewportTopIter::of(view);
    let mut end = start;
    if !end.ends_line() {
        end.forward_to_line_end();
    }
    crate::saferizer::BufferText::of_range(&view.buffer(), &start, &end).into_string()
}

/// A mark on the line the reader is currently looking at, so that line can be found
/// again after the toggle has moved every offset below the splice.
///
/// Left gravity, which is not a coin toss: the splice inserts strictly ABOVE this
/// position, so no insertion happens AT the mark and gravity cannot separate the two —
/// but a left-gravity mark also stays put if a future caller ever splices at the
/// reader's own line, which is the direction that would silently move it.
pub(super) fn anchor_reader(view: &CodePreviewView) -> gtk::TextMark {
    let iter = crate::saferizer::viewport::ViewportTopIter::of(view);
    view.buffer().create_mark(None, &iter, true)
}

/// How far `mark`'s line sits below the top of the viewport, in pixels, together with
/// that line's text — the text being what tells a moved anchor from a destroyed one.
pub(super) fn reader_offset(
    view: &CodePreviewView,
    adjustment: &gtk::Adjustment,
    mark: &gtk::TextMark,
) -> (f64, String) {
    let buf = view.buffer();
    let iter = buf.iter_at_mark(mark);
    let (y, _height) = view.line_yrange(&iter);
    let mut end = iter;
    if !end.ends_line() {
        end.forward_to_line_end();
    }
    (
        f64::from(y) - adjustment.value(),
        crate::saferizer::BufferText::of_range(&buf, &iter, &end).into_string(),
    )
}
