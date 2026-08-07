//! The scroll-position write seam.

use gtk::prelude::*;

/// Put a scroller at `value` **now**, superseding whatever the toolkit was doing
/// with it.
///
/// # Contract
///
/// A plain `GtkAdjustment` value write is not a scalar store. It is an
/// *unconditional supersede*: `gtk_adjustment_set_value` is
/// `set_value_internal(…, animate = FALSE)`, whose else-branch calls
/// `gtk_adjustment_end_updating` (gtkadjustment.c:529-530) — so it **cancels any
/// scroll animation currently in flight on that adjustment**, wherever that
/// animation had reached. Every `scroll_to_mark` / `scroll_to_iter` /
/// `scroll_mark_onscreen` scrolls by animating over ~200 ms, so "somebody else's
/// scroll is in flight" is a live possibility on any adjustment a `GtkTextView`
/// owns, and truncating it leaves the reader part-way to a destination they asked
/// for (ScrAP-260, where GTK does this to *itself* from its validation idle).
///
/// **You cannot ask whether an animation is running.** GTK's own guard for this,
/// `gtk_adjustment_is_animating` — the test `gtk_text_view_size_allocate` uses at
/// gtktextview.c:4656-4660 — is declared in `gtkadjustmentprivate.h`: not public
/// GTK API, not bound in gtk-rs. There is no supported way for an application to
/// make the write conditional, which is precisely why the decision has to be taken
/// at the call site, deliberately, rather than discovered later.
///
/// So: call this when you mean **"the position is now this, regardless"** — a
/// restore, a re-anchor, a jump the user just asked for. Do not reach for it as a
/// nudge alongside a scroll somebody else owns; two drivers over one adjustment is
/// its own trap (ScrAP-149).
///
/// GTK clamps `value` into `[lower, upper - page_size]` internally, so callers need
/// not pre-clamp for safety — only where they go on to *read the value back* and
/// want their own bound.
///
/// # Enforcement
///
/// `gtk4::prelude::AdjustmentExt::set_value` is banned crate-wide in `clippy.toml`;
/// the single `#[allow]` is below.
pub(crate) fn jump(adjustment: &gtk::Adjustment, value: f64) {
    #[allow(clippy::disallowed_methods)]
    adjustment.set_value(value);
}

/// Re-describe a scroller's whole range, keeping `value` where the caller says.
///
/// # Contract
///
/// The mirror image of [`jump`], and the reason both are banned. `gtk_adjustment_configure`
/// (gtkadjustment.c:851-854) — and `gtk_adjustment_clamp_page` (:896, :901) — write
/// `priv->value` **directly**, without the `end_updating` that [`jump`] goes through. So
/// they do *not* stop a scroll animation in flight: the animation's next frame recomputes
/// `source + t·(target − source)` and **overwrites the value that was just written**. From
/// the call site the write simply does not stick, silently, and only while an animation
/// happens to be running — so it is a defect that appears under timing and vanishes under
/// inspection.
///
/// Use this only for what it is for: republishing a range whose *geometry* changed (a
/// viewport resize, a content re-measure). If you mean "put the position here", call
/// [`jump`], which supersedes rather than races.
///
/// # Enforcement
///
/// `gtk4::prelude::AdjustmentExt::configure` and `::clamp_page` are banned crate-wide in
/// `clippy.toml`; the single `#[allow]` is below.
#[allow(clippy::too_many_arguments)]
pub(crate) fn reconfigure(
    adjustment: &gtk::Adjustment,
    value: f64,
    lower: f64,
    upper: f64,
    step_increment: f64,
    page_increment: f64,
    page_size: f64,
) {
    #[allow(clippy::disallowed_methods)]
    adjustment.configure(
        value,
        lower,
        upper,
        step_increment,
        page_increment,
        page_size,
    );
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    /// The clamping claim the doc comment makes, which a caller that reads the
    /// value back depends on. It lives here rather than in a plain `#[test]`
    /// because even a bare `GtkAdjustment` asserts GTK is initialised
    /// (gtk4-0.10.3 adjustment.rs:35) — "it is only a value holder" is not the
    /// same as "it needs no GTK" (GTK4Rs/AP-71).
    #[gtktest::test]
    fn gtk_clamps_the_written_value_into_the_scrollable_range() {
        let adjustment = gtk::Adjustment::new(0.0, 0.0, 1000.0, 1.0, 100.0, 200.0);
        jump(&adjustment, 5000.0);
        assert_eq!(
            adjustment.value(),
            800.0,
            "GTK clamps to upper - page_size, so a caller need not pre-clamp for safety"
        );
        jump(&adjustment, -5000.0);
        assert_eq!(adjustment.value(), 0.0, "and to lower at the other end");
    }

    /// **A plain write cancels a scroll animation in flight.**
    ///
    /// This is the contract the seam exists to state, and it is not obvious from
    /// either function's name — which is exactly why it needs pinning rather than
    /// describing. `gtk_adjustment_animate_to_value` is not bound in gtk-rs, so the
    /// animation is started the way the application actually starts one: a
    /// `GtkTextView` scroll.
    #[gtktest::test]
    fn a_plain_write_truncates_a_scroll_animation() {
        let view = gtk::TextView::new();
        let body: String = (0..400).map(|i| format!("line {i}\n")).collect();
        view.buffer().set_text(&body);
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_child(Some(&view));
        let window = gtk::Window::new();
        window.set_default_size(400, 300);
        window.set_child(Some(&scroller));
        window.present();

        let ctx = gtk::glib::MainContext::default();
        for _ in 0..400 {
            ctx.iteration(false);
        }
        let adjustment = scroller.vadjustment();
        let bottom = adjustment.upper() - adjustment.page_size();
        assert!(
            bottom > 100.0,
            "precondition: the document must be scrollable"
        );

        // Start an animated scroll to the far end...
        let end = view.buffer().end_iter();
        let mark = view.buffer().create_mark(None, &end, true);
        #[allow(clippy::disallowed_methods)] // starting the animation IS the fixture
        view.scroll_to_mark(&mark, 0.0, true, 0.0, 0.5);
        // ...let it get under way, but nowhere near finished (it runs ~200 ms).
        for _ in 0..3 {
            ctx.iteration(false);
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        let mid_flight = adjustment.value();

        // ...then write a position. The animation must be dead, not merely nudged:
        // its target was `bottom`, so if it were still running it would keep going.
        jump(&adjustment, 0.0);
        for _ in 0..40 {
            ctx.iteration(false);
            std::thread::sleep(std::time::Duration::from_millis(16));
        }

        assert_eq!(
            adjustment.value(),
            0.0,
            "a plain write must SUPERSEDE the animation, not be overwritten by its \
             remaining frames — it was at {mid_flight:.0} heading for {bottom:.0}"
        );
        window.destroy();
    }
}
