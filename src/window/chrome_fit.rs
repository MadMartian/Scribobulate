//! Keep width-sensitive bottom-right chrome inside the visible monitor when the
//! toolbar's content-derived minimum width forces the window wider than its screen.
//!
//! The toolbar is a single non-wrapping row whose measured minimum
//! (~1633px with every section shown) sets the window's content-derived minimum
//! width (invariant I5 — deliberate; see [`super::viewactions`]).
//! On a display narrower than that minimum the toplevel is forced wider than the
//! monitor, so anything anchored to the window's right edge — the bottom-right
//! conflict/reload/info toast's action buttons, and the right-aligned footer Ln/Col
//! indicator (TDD 9.21) — is pushed past the visible screen edge.
//!
//! There is no purely-within-window remedy: the window *itself* extends beyond the
//! monitor, so the only way to keep right-anchored chrome visible is to pull it back
//! onto the visible area, which means knowing how far the window overflows. This
//! module isolates that one geometric decision as a pure function (unit-tested
//! without a display) behind a thin GTK glue helper, so the callers just say "keep
//! this widget on screen".

use gtk::prelude::*;

/// How many logical pixels the window overflows its monitor on the right — i.e. the
/// extra right-margin needed to pull a right-anchored widget back onto the visible
/// area. Zero whenever the window fits its monitor.
///
/// Pure, so it is unit-testable without a display. Because it clamps at zero,
/// applying it is a no-op on any display wide enough to hold the toolbar (the
/// overwhelmingly common case): the widget keeps its designed margin and there is no
/// behavioural change there. It only ever acts on the too-narrow-screen case K
/// describes.
///
/// **Assumption:** the window's left edge sits at (or near) the monitor's left edge,
/// so the overflow is entirely on the right. That is how a stock X11 WM (kwin) places
/// a window whose requested minimum exceeds the monitor, and it matches the symptom
/// two independent test agents observed (K: the *right-hand* chrome went off-screen,
/// reachable "only by shifting the whole window left"). GTK4 deliberately exposes no
/// toplevel screen position (client-side positioning is gone), so a fully
/// position-independent clamp is not available; were a compositor instead to centre
/// the over-wide window, this would under-correct (chrome still partly clipped) but
/// never over-correct, and it never regresses the fits-on-screen case.
pub(crate) fn overflow_inset(win_width: i32, monitor_width: i32) -> i32 {
    (win_width - monitor_width).max(0)
}

/// Set `widget`'s right margin to `base + overflow_inset(window, monitor)` so it stays
/// within the visible monitor when the window is forced wider than the screen. On any
/// display wide enough to hold the toolbar the inset is 0 and the margin is exactly
/// `base` — the widget's designed placement, unchanged.
///
/// Recomputed at each point the chrome is (re)shown or updated (the toast show paths
/// and every footer position refresh) rather than wired to a resize signal: the
/// over-wide condition is essentially static per session (it follows the toolbar
/// min-width, I5), and driving it off the existing update points keeps this off the
/// layout hot path with no new signal plumbing. `set_margin_end` early-returns on an
/// unchanged value, so the steady state (inset 0) costs nothing. The one gap this
/// leaves — a resize while the persistent conflict toast is already up — self-heals
/// the next time the toast is shown, and never clips worse than the status quo.
pub(crate) fn apply_visible_area_inset(widget: &impl IsA<gtk::Widget>, base: i32) {
    let inset = window_monitor_widths(widget)
        .map(|(win_w, mon_w)| overflow_inset(win_w, mon_w))
        .unwrap_or(0);
    widget.set_margin_end(base + inset);
}

/// The widget's toplevel-surface width and the width of the monitor it sits on, both
/// in logical (application) pixels so they are directly comparable — or `None` before
/// the window is realized/mapped (no surface yet) or if GDK can't resolve the monitor,
/// in which case callers fall back to the base margin (i.e. current behaviour).
///
/// `gdk::Surface::width` (the client-area width GTK allocates, the thing that
/// overflows) and `gdk::Monitor::geometry` are both in logical pixels, so no
/// scale-factor reconciliation is needed.
fn window_monitor_widths(widget: &impl IsA<gtk::Widget>) -> Option<(i32, i32)> {
    let surface = widget.native()?.surface()?;
    let win_w = surface.width();
    if win_w <= 0 {
        return None;
    }
    let mon_w = widget
        .display()
        .monitor_at_surface(&surface)?
        .geometry()
        .width();
    if mon_w <= 0 {
        return None;
    }
    Some((win_w, mon_w))
}

#[cfg(test)]
mod tests {
    use super::overflow_inset;

    #[test]
    fn no_inset_when_window_fits_monitor() {
        // Typical desktop: the toolbar minimum (~1633) fits inside a 1920 monitor.
        assert_eq!(overflow_inset(1633, 1920), 0);
    }

    #[test]
    fn no_inset_when_window_exactly_fills_monitor() {
        assert_eq!(overflow_inset(1600, 1600), 0);
    }

    #[test]
    fn inset_equals_overflow_when_window_wider_than_monitor() {
        // The reported case: a 1633-wide window on a 1400 display overflows by 233,
        // so right-anchored chrome must be pulled 233px left to stay visible.
        assert_eq!(overflow_inset(1633, 1400), 233);
    }

    #[test]
    fn inset_never_negative() {
        // A window far narrower than its monitor never produces a negative margin
        // (which would push chrome off the *right* instead).
        assert_eq!(overflow_inset(800, 1920), 0);
    }
}
