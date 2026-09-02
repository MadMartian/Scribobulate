//! Waiting for a `GtkTextView`'s scroll position to stop being written to.
//!
//! [`super::after_line_heights_validated`] answers *"is the layout valid?"*, which is
//! the right question for issuing a scroll. This module answers the neighbouring one —
//! *"has GTK finished moving the viewport on its own?"* — which is what a caller must
//! ask before **restoring** a reading position it recorded before an edit.
//!
//! # The defect this exists for
//!
//! An edit ABOVE the viewport makes `GtkTextView` compensate: `changed_handler`
//! (gtktextview.c:4918-4925, GTK 4.6.9) recomputes the first paragraph's top and writes
//! the difference back through `gtk_adjustment_set_value`, once per `GtkTextLayout`
//! `::changed` emission the edit provokes. On GTK before **4.19.3** each of those
//! passes lands `top_margin` pixels short — the handler reads `yoffset = value −
//! top_margin` correctly and then hands `set_value` a `yoffset` where an adjustment
//! VALUE is wanted — so the reader drifts by `emissions × top_margin`. Fixed upstream
//! by commit `b300698629` (GNOME/gtk#4134); MEASURED here at 32 / 368 / 880 px for
//! 0 / 10 / 30 anchored children in a spliced region, at a 16 px margin.
//!
//! **The corrective is to RESTORE, never to compensate**, and that is a measurement
//! rather than a preference: the emission count is BIMODAL at a fixed dose (55 or 63
//! for the identical toggle, same margin, run to run), so no fixed correction is right
//! more than half the time — and any correction would become a *double* correction on
//! 4.19.3+. A restore that recomputes the reader's offset from the settled geometry
//! never counts emissions at all, so the bimodality cannot reach it, and it degrades
//! to a no-op on a fixed GTK because there is nothing left to undo.
//!
//! # Why the wait has to be a QUIESCENCE and not a delay
//!
//! ⚠ `gtk_text_view_value_changed` **destroys `first_validate_idle`** (:8437-8443),
//! which is the only thing that ever consumes a `GtkTextView`'s pending scroll
//! (ScrAP-260). Every one of those compensating passes calls `set_value`, so every one
//! of them orphans a scroll queued while the settle is running: **a restore issued
//! during the settle is silently eaten.** It must land strictly after the last write.
//!
//! And the last write cannot be predicted. The emission count is dose-dependent —
//! MEASURED 2 at zero anchored children in the region and 55 at thirty — so a fixed
//! idle, at any priority, is tuned to whichever document it was written against and
//! passes on that one. [`after_scroll_settles`] therefore triggers on the writes
//! themselves going quiet, exactly as [`super::after_line_heights_validated`]'s own
//! deadline triggers on stalled PROGRESS rather than on elapsed time.
//!
//! # Both conditions, because neither implies the other
//!
//! * **The layout-valid oracle alone is not enough.** Its own rustdoc records the hole:
//!   a main loop pumped from *inside* `incremental_validate_callback`'s stack can
//!   dispatch the 200-priority idle with the layout still invalid, and anchored-child
//!   allocation is the realistic route — which a spliced disclosure body is full of.
//!   Allocation also runs on the frame clock, so a compensating write can arrive after
//!   the last validation idle has been dispatched.
//! * **Emission quiet alone is not enough.** `GtkAdjustment` swallows a set to a value
//!   it already holds, so a `::changed` computing a zero compensation emits NOTHING
//!   (`preview::splice::excursion::recorder` records this as the instrument's one
//!   lossy direction). A lull is therefore not proof the storm is over, and firing
//!   inside one restores against a half-validated `line_yrange`.
//!
//! So the gate is the conjunction, with an absolute tick cap **on top of** the
//! stalled-progress term rather than in place of it — the two bound different failures.
//! Stalled progress is what ends the ordinary case promptly; the tick cap is the
//! backstop for a storm that never goes quiet at all. That is the same "promptly when it
//! settles, late and against partial geometry when it never does" degradation
//! [`super::after_line_heights_validated`] already chose. Stated this way because the
//! sentence above says the wait triggers on writes GOING QUIET and this one used to say
//! the bound WAS the tick cap, which reads as the quiet term having been replaced.

use gtk::glib;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

/// How often the settle check samples the adjustment's write count.
///
/// Short relative to the compensation storm it watches (which runs off back-to-back
/// validation idles), long enough that [`SETTLE_QUIET_TICKS`] spans several frames —
/// the writes driven by `size_allocate` arrive on the frame clock, not on an idle, so
/// a sub-frame quiet window would call a gap between frames a settle.
const SETTLE_TICK: Duration = Duration::from_millis(50);

/// Consecutive ticks with no adjustment write before the viewport counts as quiet.
///
/// Three, so the quiet window is ~150 ms — comfortably more than one 60 Hz frame, and
/// the reason it is expressed in ticks rather than as a duration is that the tick is
/// also what guarantees the loop keeps turning while the main loop is otherwise idle.
const SETTLE_QUIET_TICKS: u32 = 3;

/// Absolute bound, for a view whose geometry never settles at all.
///
/// A FAILURE bound (GTK4Rs/AP-122), never the completion signal: at
/// [`SETTLE_TICK`] this is ten seconds, which a healthy document is nowhere near.
const SETTLE_MAX_TICKS: u32 = 200;

/// Run `f` once GTK has stopped moving `view`'s viewport by itself.
///
/// **Contract.** `f` runs at most once, on the main loop, at a point where `view`'s
/// line heights are validated AND its vertical adjustment has been left alone for
/// [`SETTLE_QUIET_TICKS`] — so `line_yrange` reports real geometry and a `set_value`
/// issued from `f` is not about to be overwritten by GTK's own compensation. `f` does
/// not run at all if the view has been dropped or unrealized in the meantime.
///
/// **Precondition.** None. On a view nothing is happening to, `f` runs after the
/// quiet window rather than immediately — the wait is a property of the mechanism, not
/// of the caller's state.
///
/// **Not for issuing a navigation.** Use [`super::scroll_to_mark_when_ready`] to go
/// somewhere; this is for putting the reader back where they already were, which is
/// the case that must not race GTK's own writes.
pub(crate) fn after_scroll_settles<F>(view: &gtk::TextView, f: F)
where
    F: FnOnce(&gtk::TextView) + 'static,
{
    // Weak capture: a strong one would pin the view alive as an unrooted zombie after
    // `window.destroy()` and fire against it (GTK4Rs/AP-128). `upgrade()` is
    // liveness-only, so the realized state is gated separately below.
    let weak = view.downgrade();
    let once: Rc<RefCell<Option<F>>> = Rc::new(RefCell::new(Some(f)));

    let layout_valid = Rc::new(Cell::new(false));
    {
        let valid = Rc::clone(&layout_valid);
        super::after_line_heights_validated(view, move |_| valid.set(true));
    }

    // The instrument: every write to this view's vertical adjustment, whoever made it.
    // GTK's compensating writes are the ones this is watching for, but counting ALL of
    // them is deliberate — a split-pane sync or an outline navigation landing mid-settle
    // is equally a reason not to restore yet, and distinguishing them would need a flag
    // that every future writer would have to remember to set.
    let writes = Rc::new(Cell::new(0u64));
    let armed: Rc<RefCell<Option<(gtk::Adjustment, glib::SignalHandlerId)>>> =
        Rc::new(RefCell::new(None));
    if let Some(adjustment) = view.vadjustment() {
        let writes = Rc::clone(&writes);
        // The handler takes its emitter as an argument and captures no widget, so it
        // closes no reference cycle (GTK4Rs/AP-63).
        let id = adjustment.connect_value_changed(move |_| writes.set(writes.get() + 1));
        *armed.borrow_mut() = Some((adjustment, id));
    }
    let disarm = {
        let armed = Rc::clone(&armed);
        move || {
            if let Some((adjustment, id)) = armed.borrow_mut().take() {
                adjustment.disconnect(id);
            }
        }
    };

    let mut last_seen = 0u64;
    let mut quiet_ticks = 0u32;
    let mut ticks = 0u32;
    glib::timeout_add_local(SETTLE_TICK, move || {
        ticks += 1;
        let seen = writes.get();
        if seen == last_seen {
            quiet_ticks += 1;
        } else {
            quiet_ticks = 0;
            last_seen = seen;
        }
        let alive = weak.upgrade();
        if alive.is_some() && !settle_should_fire(layout_valid.get(), quiet_ticks, ticks) {
            return glib::ControlFlow::Continue;
        }
        // Disarmed BEFORE `f` runs, so the restore's own `set_value` is not counted by
        // an instrument that is about to be thrown away anyway — and so a caller that
        // re-arms this wait from inside `f` starts from a clean count rather than
        // inheriting the write it just made.
        disarm();
        if let (Some(view), Some(f)) = (alive, once.borrow_mut().take()) {
            // A zombie retains its last allocation, so no geometry check can tell it
            // from a live view — `is_realized()` is the exact test (GTK4Rs/AP-128).
            if view.is_realized() {
                f(&view);
            }
        }
        glib::ControlFlow::Break
    });
}

/// Whether the deferred restore may run on this tick.
///
/// Split out of the timer closure so the conjunction is pinned by tests rather than
/// only by prose — both operands read as redundant from the other's side, which is
/// exactly the shape a later "simplification" deletes one of (GTK4Rs/AP-254):
///
/// * dropping `layout_valid` fires inside a lull between validation chunks, restoring
///   against a `line_yrange` that has not been computed yet;
/// * dropping the quiet window fires while GTK is still compensating, and the restore
///   is then simply overwritten (or, if it went through `scroll_to_mark`, destroyed
///   outright — ScrAP-260).
fn settle_should_fire(layout_valid: bool, quiet_ticks: u32, ticks: u32) -> bool {
    (layout_valid && quiet_ticks >= SETTLE_QUIET_TICKS) || ticks >= SETTLE_MAX_TICKS
}

#[cfg(test)]
mod decision_tests {
    use super::*;

    #[test]
    fn a_settled_layout_that_has_been_quiet_long_enough_fires() {
        assert!(settle_should_fire(true, SETTLE_QUIET_TICKS, 4));
    }

    #[test]
    fn a_valid_layout_still_being_written_to_keeps_waiting() {
        // The compensation storm: the layout can report valid while
        // `size_allocate`-driven writes are still arriving on the frame clock.
        assert!(!settle_should_fire(true, 0, 4));
        assert!(!settle_should_fire(true, SETTLE_QUIET_TICKS - 1, 4));
    }

    #[test]
    fn a_quiet_adjustment_over_an_invalid_layout_keeps_waiting() {
        // `GtkAdjustment` swallows a same-value set, so a `::changed` computing a zero
        // compensation emits nothing — quiet is not proof the storm is over, and
        // restoring here would read a `line_yrange` that is still the validation
        // frontier.
        assert!(!settle_should_fire(false, SETTLE_QUIET_TICKS * 4, 40));
    }

    #[test]
    fn the_tick_cap_fires_even_with_neither_condition_met() {
        // A view whose geometry never settles degrades to a late restore against
        // partial geometry rather than to an unbounded wait.
        assert!(settle_should_fire(false, 0, SETTLE_MAX_TICKS));
    }

    #[test]
    fn the_tick_cap_is_a_floor_not_an_equality() {
        // Guards against a `==` a stray extra tick would step straight over.
        assert!(settle_should_fire(false, 0, SETTLE_MAX_TICKS + 1));
    }

    #[test]
    fn the_quiet_bar_is_a_floor_too() {
        assert!(settle_should_fire(true, SETTLE_QUIET_TICKS + 7, 12));
    }
}
