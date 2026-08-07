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
//! (cf. ScrAP-105). Reserve `iter_at_location` for genuine pointer hit-tests.
//!
//! Coordinates are BUFFER coordinates — the space `visible_rect` and
//! `snapshot_layer` already work in; no window-coordinate translation is applied.

use gtk::prelude::*;

/// The top-of-viewport iter read (see module contract).
pub(crate) struct ViewportTopIter;

impl ViewportTopIter {
    /// The iter at the line occupying the top of `view`'s viewport.
    pub(crate) fn of(view: &impl IsA<gtk::TextView>) -> gtk::TextIter {
        let view: &gtk::TextView = view.as_ref();
        let (top, _) = view.line_at_y(view.visible_rect().y());
        top
    }

    /// The char offset of [`Self::of`] — the common caller need.
    pub(crate) fn top_offset(view: &impl IsA<gtk::TextView>) -> i32 {
        Self::of(view).offset()
    }
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
    pub(crate) fn of(view: &impl IsA<gtk::TextView>) -> Self {
        let view: &gtk::TextView = view.as_ref();
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

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod tests {
    use super::*;
    use gtk::glib;

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
        // real content extent (well past one page) after several idles. Bounded so
        // an idle display can never block forever.
        let ctx = glib::MainContext::default();
        let vadj = view.vadjustment().expect("vadjustment");
        let mut ready = false;
        for _ in 0..400 {
            ctx.iteration(false);
            std::thread::sleep(std::time::Duration::from_millis(4));
            if view.is_mapped() && view.height() > 0 && vadj.upper() > vadj.page_size() * 3.0 {
                ready = true;
                break;
            }
        }
        assert!(ready, "view mapped and full document height validated");

        // Scroll well past the top so the viewport-top line is NOT line 0. With the
        // document height validated, `upper` is the real content extent (not the
        // draft page height), so a direct `set_value` moves the viewport
        // deterministically — no deferred, one-shot `scroll_to_iter` that would land
        // pre-validation at the top (GTK4Rs/AP-115/22).
        crate::saferizer::scrollpos::jump(&vadj, vadj.upper() * 0.5);
        for _ in 0..50 {
            ctx.iteration(false);
            std::thread::sleep(std::time::Duration::from_millis(4));
        }

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
