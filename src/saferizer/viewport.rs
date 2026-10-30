//! `ViewportTopIter` / `ViewportRange` — the top-of-viewport (and full visible
//! range) `GtkTextView` read, centralized (GTK4Rs/AP-15).
//!
//! **Contract.** The iter at the top of a scrolled `GtkTextView`'s viewport must
//! be derived from `visible_rect().y()` + `line_at_y(y)`, NOT `iter_at_location`.
//! `iter_at_location` is a glyph hit-test: at x = 0 (the left edge) or anywhere in
//! the view's margins it lands on no glyph and returns `None`, so it cannot name
//! the line that merely *starts* the viewport. `line_at_y` maps a y-coordinate to
//! the line occupying it with no glyph requirement — correct at the margins — and
//! without touching the line-display cache, so it is also safe to call mid-paint
//! (cf. GTK4Rs/AP-89). Reserve `iter_at_location` for genuine pointer hit-tests.
//!
//! Coordinates are BUFFER coordinates — the space `visible_rect` and
//! `snapshot_layer` already work in; no window-coordinate translation is applied.
//!
//! **Second half of the contract: there must BE a viewport.** Before the view has
//! been allocated there is no layout for `line_at_y` to consult, and it does not
//! decline — it answers the **last line of the buffer**, for *every* y. MEASURED
//! (GTK 4.6.9, X11/Xvfb, 201-line buffer): an unrealized `GtkTextView` reports
//! `visible_rect() == (0, 0, 0×0)` and answers `200` for y ∈ {−100, 0, 1, 50, 500,
//! 5000, 100000}; once mapped and allocated the same calls return 0, 0, 2 and 200
//! respectively. So the failure is not imprecision — it is the *opposite end of the
//! document*, it is silent, and it is not confined to the y = 0 top-of-viewport
//! read. Every read below therefore gates on the view having a real allocation and
//! answers "the top of the buffer" when it does not, which is what a view with no
//! layout is showing. GTK4Rs/AP-263.
//!
//! Two things that trap a reader who tries to shortcut this:
//!
//! * **You cannot detect it from the returned value.** An *allocated* view also
//!   answers with the last line for a y past the end — correctly. "It returned the
//!   last line" is therefore not diagnostic; only the allocation state is, which is
//!   why the gate is here at the boundary and not a plausibility check on the result.
//! * **`has_viewport` is not a liveness check.** It answers "has a layout ever been
//!   computed", and once one has, it stays true through unparent, reparent into an
//!   unmapped scroller, and the toplevel being destroyed (MEASURED: all three keep
//!   reporting the stale allocated height, while `is_realized()` flips to false).
//!   That is fine here — the dangerous window is strictly *before* the first
//!   allocation, and a torn-down view's cached layout answers merely stale rather
//!   than far-end wrong. But anything acting on this read *after a deferral* needs
//!   `is_realized()` as well (GTK4Rs/AP-128's weak-capture + realize gate).
//!
//! `visible_rect().height()` is the signal used rather than the vadjustment's
//! `page_size`: the two were measured moving in lockstep across all six
//! allocation/teardown shapes, so they are interchangeable for this question, and
//! `visible_rect` wins only on locality — it needs no adjustment, and therefore no
//! `ScrolledWindow`, so the seam stays usable on a bare view.

use gtk::prelude::*;

/// Whether `view` has an allocation for `line_at_y` to be answerable against.
/// See the module contract's second half — a `0×0` visible rect means no layout,
/// and every geometry read past this point would be answering about nothing.
pub(crate) fn has_viewport(view: &gtk::TextView) -> bool {
    view.visible_rect().height() > 0
}

/// The top-of-viewport iter read (see module contract).
pub(crate) struct ViewportTopIter;

impl ViewportTopIter {
    /// The iter at the line occupying the top of `view`'s viewport — or the start
    /// of the buffer when `view` has no viewport yet (module contract, GTK4Rs/AP-263).
    pub(crate) fn of(view: &impl IsA<gtk::TextView>) -> gtk::TextIter {
        let view: &gtk::TextView = view.as_ref();
        if !has_viewport(view) {
            return view.buffer().start_iter();
        }
        let (top, _) = view.line_at_y(view.visible_rect().y());
        top
    }

    /// The char offset of [`Self::of`] — the common caller need.
    pub(crate) fn top_offset(view: &impl IsA<gtk::TextView>) -> i32 {
        Self::of(view).offset()
    }

    /// The vertical-adjustment value that puts buffer `y` at the top of the viewport.
    ///
    /// **Not `y`.** The adjustment's range includes the view's `top_margin`, so the
    /// viewport's top edge is at buffer `value − top_margin` (`visible_rect().y()`,
    /// which [`Self::of`] reads). Writing a line's `line_yrange` y straight into the
    /// adjustment parks that line `top_margin` pixels BELOW the top edge, and the line
    /// at the top is the one before it. MEASURED (GTK 4.6.9, the preview's 16 px
    /// margin): a live reload restoring heading 17.6 of `sdd/TDD.md` wrote 95081, the
    /// heading's y; `scroll_to_mark` had put it at 95097. The reader saw nearly the same
    /// screen, and the outline — which reads the top through this seam — moved its
    /// highlight back a section. This is the same value/y confusion GTK itself made in
    /// GTK4Rs/AP-321's compensation path.
    pub(crate) fn value_for_top_y(view: &impl IsA<gtk::TextView>, y: i32) -> f64 {
        f64::from(y) + f64::from(view.as_ref().top_margin())
    }

    /// The inverse of [`Self::value_for_top_y`]: the buffer y at the viewport's top edge
    /// when the adjustment reads `value`. For a value read DURING a `value-changed`
    /// emission, where `visible_rect` may not have caught up yet.
    pub(crate) fn top_y_for_value(view: &impl IsA<gtk::TextView>, value: f64) -> i32 {
        value as i32 - view.as_ref().top_margin()
    }
}

/// Whether a `GtkTextView` vadjustment change from `prev` to `next` moves the text on
/// screen.
///
/// The view paints at the TRUNCATED value (`gtk_text_view_value_changed`, GTK 4.6.9
/// gtktextview.c:8397, `(int) gtk_adjustment_get_value`). A wheel step leaves a fraction
/// in the value, and the view's next allocation writes the truncated value back
/// (`gtk_text_view_set_vadjustment_values`, :8331-8365). That write emits `value-changed`
/// and moves nothing, so a handler that hides something on scroll must ask this, not
/// react to the signal (GTK4Rs/AP-360).
pub(crate) fn moves_painted_text(prev: f64, next: f64) -> bool {
    prev.trunc() != next.trunc()
}

/// The full visible range of a `GtkTextView`'s viewport: the top and bottom
/// y-coordinates (buffer space) and the iter at each, both read via the
/// `line_at_y` contract documented on this module.
///
/// [`Self::bottom`] is the RAW `line_at_y(bottom_y)` result: callers that want the
/// line's *end* apply `forward_to_line_end` themselves, since whether a partially
/// visible bottom line counts as in-range is caller policy, not the read's.
pub(crate) struct ViewportRange {
    /// Top edge of the viewport, buffer y.
    pub(crate) top_y: i32,
    /// Bottom edge of the viewport, buffer y.
    pub(crate) bottom_y: i32,
    /// Iter at the line occupying `top_y`.
    pub(crate) top: gtk::TextIter,
    /// Iter at the line occupying `bottom_y`.
    pub(crate) bottom: gtk::TextIter,
}

impl ViewportRange {
    /// Read `view`'s visible range in one shot (see [`ViewportRange`]).
    ///
    /// A view with no viewport yet reports an empty range at the top of the
    /// buffer rather than `line_at_y`'s last-line answer — the same gate
    /// [`ViewportTopIter::of`] takes, for the same measured reason (GTK4Rs/AP-263).
    /// An empty range is the honest description: nothing is visible.
    pub(crate) fn of(view: &impl IsA<gtk::TextView>) -> Self {
        let view: &gtk::TextView = view.as_ref();
        if !has_viewport(view) {
            let start = view.buffer().start_iter();
            return Self {
                top_y: 0,
                bottom_y: 0,
                top: start,
                bottom: start,
            };
        }
        let vis = view.visible_rect();
        let top_y = vis.y();
        let bottom_y = vis.y() + vis.height();
        let (top, _) = view.line_at_y(top_y);
        let (bottom, _) = view.line_at_y(bottom_y);
        Self {
            top_y,
            bottom_y,
            top,
            bottom,
        }
    }
}

/// Whether this GTK is one where the allocation write-back [`moves_painted_text`] guards
/// against was MEASURED: GTK 4.6.9 writes the truncated value back on the view's next
/// allocation, and GTK 4.22 (macOS and Windows CI) was measured not to. A test that
/// plants the write-back asserts it happened here, and reports a skip on any other GTK
/// rather than pass on a run that never exercised it.
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) fn truncated_writeback_measured() -> bool {
    gtk::minor_version() == 6
}

#[cfg(test)]
mod painted_text_tests {
    use super::moves_painted_text;

    /// The values are the ones measured on a wheel scroll in split view: five steps to
    /// 416.40, then the view's allocation writing back 416.
    #[test]
    fn writing_back_the_truncated_value_does_not_move_the_text() {
        assert!(!moves_painted_text(416.401_609_200_426_4, 416.0));
        assert!(!moves_painted_text(83.280_321_840_085_28, 83.0));
        assert!(!moves_painted_text(10.0, 10.0));
        assert!(!moves_painted_text(10.2, 10.9));
    }

    #[test]
    fn a_scroll_across_a_pixel_moves_the_text() {
        assert!(moves_painted_text(
            333.121_287_360_341_1,
            416.401_609_200_426_4
        ));
        assert!(moves_painted_text(416.0, 415.0));
        assert!(moves_painted_text(0.9, 1.0));
        assert!(moves_painted_text(0.0, 83.28));
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod tests {
    use super::*;

    /// GTK4Rs/AP-263 — a view with no allocation must report the TOP of the buffer,
    /// and the raw call it replaces must be shown reporting the BOTTOM, in the
    /// same body.
    ///
    /// The control is what makes this a measurement rather than an assertion of
    /// intent: without it the seam's answer of `0` is indistinguishable from a
    /// view that simply is at line 0, which is every view before it scrolls. With
    /// it, the body states the GTK behaviour it exists to absorb — `line_at_y(0)`
    /// on an unallocated view returns `line_count - 1` — so if a future GTK ever
    /// stops doing that, this fails and says so rather than quietly guarding
    /// nothing.
    #[gtktest::test]
    fn the_top_of_an_unallocated_view_is_the_start_of_the_buffer_not_its_end() {
        let buffer = gtk::TextBuffer::new(None);
        buffer.set_text("l0\nl1\nl2\nl3\nl4\nl5\nl6\n");
        let view = gtk::TextView::builder().buffer(&buffer).build();

        assert_eq!(
            view.visible_rect().height(),
            0,
            "precondition: a view that was never allocated has no viewport"
        );
        // The control — the raw read this seam exists to replace.
        let (raw, _) = view.line_at_y(view.visible_rect().y());
        assert_eq!(
            raw.line(),
            buffer.line_count() - 1,
            "GTK 4.6 answers `line_at_y` with the LAST line before allocation; if \
             that has changed, this seam's gate needs revisiting, not deleting"
        );

        assert_eq!(
            ViewportTopIter::of(&view).line(),
            0,
            "the seam must answer with the top of the buffer, which is what a view \
             with no layout is showing"
        );
        let range = ViewportRange::of(&view);
        assert_eq!(
            (range.top.line(), range.bottom.line(), range.bottom_y),
            (0, 0, 0),
            "and the range read must report an empty range at the top, not a \
             range spanning to the last line"
        );
    }

    /// The seam must return the exact iter a hand-written `visible_rect().y()` +
    /// `line_at_y` read would, on a view scrolled off line 0.
    #[gtktest::test]
    fn viewport_top_iter_matches_manual_read_on_a_scrolled_view() {
        let buffer = gtk::TextBuffer::new(None);
        let text: String = (0..500).map(|i| format!("line {i}\n")).collect();
        buffer.set_text(&text);
        let view = gtk::TextView::builder().buffer(&buffer).build();

        let sw = gtk::ScrolledWindow::builder().child(&view).build();
        let window = gtk::Window::builder()
            .default_width(400)
            .default_height(300)
            .child(&sw)
            .build();
        window.present();

        // Pump until mapped AND the full document height has been validated —
        // GtkTextView validates ~a screenful per idle, so `upper` only reaches the
        // real content extent (well past one page) after several idles.
        //
        // Through the shared pump (M31), not a hand-rolled turn count. This site used
        // to spin 400 turns with a 4 ms sleep between them, which is both halves of
        // GTK4Rs/AP-261 wrong for idle-driven work: the sleep delays a source that is
        // already runnable, and the budget it buys is a turn count rather than time —
        // ~1.6 s, which a hosted macOS runner does not always finish validating inside.
        // The failure was a red test on CI and a green one everywhere else, saying
        // nothing about the seam under test.
        let vadj = view.vadjustment().expect("vadjustment");
        crate::testpump::until(
            crate::testpump::Clock::Idle,
            "the view to map and GTK to validate the full document height",
            {
                let view = view.clone();
                let vadj = vadj.clone();
                move || {
                    view.is_mapped() && view.height() > 0 && vadj.upper() > vadj.page_size() * 3.0
                }
            },
        );

        // Scroll well past the top so the viewport-top line is NOT line 0. With the
        // document height validated, `upper` is the real content extent (not the
        // draft page height), so a direct `set_value` moves the viewport
        // deterministically — no deferred, one-shot `scroll_to_iter` that would land
        // pre-validation at the top (GTK4Rs/AP-115/22).
        crate::saferizer::scrollpos::jump(&vadj, vadj.upper() * 0.5);
        crate::testpump::until(
            crate::testpump::Clock::Idle,
            "the view's visible rect to follow the adjustment",
            {
                let view = view.clone();
                let vadj = vadj.clone();
                move || (f64::from(view.visible_rect().y()) - vadj.value()).abs() < 1.0
            },
        );

        // Manual GTK4Rs/AP-15 read.
        let vis = view.visible_rect();
        let (manual_top, _) = view.line_at_y(vis.y());
        assert!(
            manual_top.offset() > 0,
            "the view must actually be scrolled off line 0 for this to test anything"
        );

        assert_eq!(
            ViewportTopIter::of(&view).offset(),
            manual_top.offset(),
            "the seam must return the same top iter as the manual read"
        );
        assert_eq!(ViewportTopIter::top_offset(&view), manual_top.offset());

        // The range's top must agree too.
        let range = ViewportRange::of(&view);
        assert_eq!(range.top.offset(), manual_top.offset());
        assert_eq!(range.top_y, vis.y());

        window.destroy();
    }
}
