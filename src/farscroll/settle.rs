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
/// line heights **have been validated at least once** AND its vertical adjustment has
/// been left alone for [`SETTLE_QUIET_TICKS`] — so `line_yrange` reports real geometry
/// and a `set_value` issued from `f` is not about to be overwritten by GTK's own
/// compensation. `f` does not run at all if the view has been dropped or unrealized in
/// the meantime.
///
/// "Have been validated" rather than "are validated" is exact: the oracle is a one-way
/// latch (see [`arm_settle`]), so what carries the invariant from the latch onward is
/// the quiet window — nothing has written the adjustment for three ticks, and a
/// re-validation that moved geometry would have written it.
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
    let layout_valid = Rc::new(Cell::new(false));
    {
        let valid = Rc::clone(&layout_valid);
        super::after_line_heights_validated(view, move |_| valid.set(true));
    }
    arm_settle(view, SETTLE_TICK, layout_valid, Rc::new(Cell::new(0)), f);
}

/// The state machine [`after_scroll_settles`] IS, with its two ambient inputs — the
/// tick interval and the layout oracle — supplied rather than constructed inline.
///
/// **The write counter is the THIRD ambient input, supplied for the same reason the
/// other two are.** It is the instrument the quiet window is read from, and a test that
/// cannot read it can only assert around it: the disarm guard used to be checked by
/// arming a SECOND counter and observing that the restore's write landed, which passes
/// whether or not the first handler is still connected (F-TEST-A-002). Handed in, the
/// assertion is direct — the settle's own instrument must show the same count after
/// `f` has written the adjustment as it did at the moment it fired.
///
/// **Split out to make the wrapper drivable.** Six behaviours live here and each is a
/// plausible silent failure: `f` running at most once; the instrument being disarmed
/// BEFORE `f` runs (a caller re-arming from inside `f` depends on it); the
/// `is_realized` gate; the weak-upgrade path when the view is dropped mid-wait; a view
/// with no vertical adjustment, where the write count never moves and the quiet window
/// is therefore satisfied by a FALSE quiet; and the tick cap. With the real 50 ms tick
/// and the real validation oracle hard-coded, none of them could be asserted except
/// through an end-to-end pixel reading that names the reader's position rather than
/// which behaviour broke. A test drives this at a 1 ms tick with the latch set by hand.
///
/// `layout_valid` is a ONE-WAY LATCH: [`super::after_line_heights_validated`] sets it
/// once and nothing clears it, so a layout that goes invalid again after the oracle
/// fired leaves the conjunction resting on the quiet window alone. That is deliberate —
/// a re-arming oracle would restart the wait every time GTK re-validated a chunk, which
/// on a long document is never — but it means the contract's "line heights are
/// validated" is precisely *"have been validated at least once, and nothing has written
/// the adjustment since"*, and the quiet window is what carries the invariant from
/// there.
fn arm_settle<F>(
    view: &gtk::TextView,
    tick: Duration,
    layout_valid: Rc<Cell<bool>>,
    writes: Rc<Cell<u64>>,
    f: F,
) where
    F: FnOnce(&gtk::TextView) + 'static,
{
    // Weak capture: a strong one would pin the view alive as an unrooted zombie after
    // `window.destroy()` and fire against it (GTK4Rs/AP-128). `upgrade()` is
    // liveness-only, so the realized state is gated separately below.
    let weak = view.downgrade();
    let once: Rc<RefCell<Option<F>>> = Rc::new(RefCell::new(Some(f)));

    // The instrument: every write to this view's vertical adjustment, whoever made it.
    // GTK's compensating writes are the ones this is watching for, but counting ALL of
    // them is deliberate — a split-pane sync or an outline navigation landing mid-settle
    // is equally a reason not to restore yet, and distinguishing them would need a flag
    // that every future writer would have to remember to set.
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
    glib::timeout_add_local(tick, move || {
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
        // The SHARED gate — see `farscroll::run_once_if_live`. This module and its
        // parent each spelled the upgrade / take / `is_realized` sequence out, which is
        // one place too many for a rule whose omission fails by running the caller's
        // work against a destroyed view (F-DRY-A-011).
        super::run_once_if_live(alive, &once);
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

/// The wrapper around [`settle_should_fire`], driven directly.
///
/// [`arm_settle`] exists so these six behaviours are assertable individually. Before it,
/// the only witness that any of them worked was `preview::splice::excursion::wired`'s
/// end-to-end pixel reading — which names the READER'S POSITION when it fails, not which
/// of the six broke. Each body here runs at a 1 ms tick with the layout oracle set by
/// hand, so none takes meaningfully longer than a pump.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;
    use crate::testpump::{until, until_for, Clock};

    /// Fast enough that `SETTLE_QUIET_TICKS` and `SETTLE_MAX_TICKS` are reached inside a
    /// pump rather than in ten real seconds — the whole reason the tick is a parameter.
    const FAST: Duration = Duration::from_millis(1);

    /// A REALIZED view in a presented window, which is what the `is_realized` gate
    /// requires; and a latch already set, because the layout oracle is not what these
    /// bodies are about.
    ///
    /// The buffer is deliberately TALLER THAN THE WINDOW. An empty view's vadjustment
    /// has `upper == page_size`, so every write to it clamps back to zero and emits
    /// nothing — which would make the disarm test's own precondition unsatisfiable while
    /// looking like a defect in the code under test.
    ///
    /// **And the view is hosted in a `ScrolledWindow`, which is not decoration — it is
    /// what BOUNDS `page_size`.** A bare `GtkTextView` in a `GtkWindow` is eventually
    /// allocated its full natural height, so `page_size` grows until it equals `upper`,
    /// the range collapses to zero, and `gtk_adjustment_sanitize_value`'s
    /// `CLAMP (value, lower, upper - page_size)` becomes `CLAMP (v, 0, 0)` — every write
    /// silently becomes 0. The wait below is then satisfied only TRANSIENTLY: it samples a
    /// mid-layout `page_size` that has not finished growing.
    ///
    /// **MEASURED, and it cost an investigation.** On macOS the collapse lands inside the
    /// settle's own window: bare gave 3 pass / 17 fail against 20 pass / 0 fail hosted,
    /// interleaved under matched load. On Linux the bare geometry is stable from t=0
    /// (alloc 400×2322, `page_size` 2322 against `upper` 7218) so it never failed here,
    /// which is why a fixture defect looked like a platform defect for as long as it did —
    /// and a single stale `page_size` sample, reported as "no clamp available to explain
    /// it", sent three of us hunting a competing writer inside GTK that did not exist.
    /// The scroller also makes the fixture match how the product actually hosts this view
    /// (`preview::render`), which is the reason it should have been here from the start.
    fn presented_view() -> (gtk::TextView, gtk::Window, Rc<Cell<bool>>) {
        let view = gtk::TextView::new();
        let body: String = (0..400).map(|i| format!("line {i}\n")).collect();
        view.buffer().set_text(&body);
        let scroller = gtk::ScrolledWindow::builder().child(&view).build();
        let window = gtk::Window::new();
        window.set_default_size(400, 300);
        window.set_child(Some(&scroller));
        window.present();
        until(
            Clock::Idle,
            "the view to realize with a scrollable range",
            {
                let view = view.clone();
                move || {
                    view.is_realized()
                        && view
                            .vadjustment()
                            .is_some_and(|a| a.upper() > a.page_size() && a.page_size() > 0.0)
                }
            },
        );
        (view, window, Rc::new(Cell::new(true)))
    }

    /// **`f` runs exactly once**, however long the loop keeps turning afterwards.
    ///
    /// The timer returns `Break`, so a second run would need the source to survive it —
    /// but the `once.take()` is what makes the contract independent of that, and it is
    /// the part a refactor can drop while the test still "passes" on the source's
    /// behaviour alone. Counted rather than asserted-once so a second call is a
    /// FAILURE and not merely unobserved.
    #[gtktest::test]
    fn the_deferred_work_runs_exactly_once() {
        let (view, window, valid) = presented_view();
        let runs = Rc::new(Cell::new(0u32));
        arm_settle(&view, FAST, valid, Rc::new(Cell::new(0)), {
            let runs = Rc::clone(&runs);
            move |_| runs.set(runs.get() + 1)
        });
        until(Clock::Frame, "the settle to fire", {
            let runs = Rc::clone(&runs);
            move || runs.get() > 0
        });
        // Keep the loop turning well past the tick, so a source that failed to break
        // has every chance to fire again.
        crate::testpump::drain_for(Clock::Frame, Duration::from_millis(50));
        assert_eq!(runs.get(), 1, "the deferred work runs at most once");
        window.destroy();
    }

    /// **The write instrument is disarmed BEFORE `f` runs**, so a caller that re-arms
    /// the wait from inside `f` starts from a clean count rather than inheriting the
    /// `set_value` it just made.
    ///
    /// **Asserted on the settle's OWN counter, which is why `arm_settle` takes one**
    /// (F-TEST-A-002). Its predecessor armed a second, independent counter and checked
    /// that the restore's write landed on the adjustment — which it does whether or not
    /// the first handler is still connected, so the test passed with `disarm()` deleted.
    /// The direct statement is that the instrument shows the same count after `f` has
    /// written as it did at the instant it fired: a live handler would have counted that
    /// write.
    #[gtktest::test]
    fn the_instrument_is_disarmed_before_the_deferred_work_runs() {
        let (view, window, valid) = presented_view();
        let adjustment = view.vadjustment().expect("a TextView has a vadjustment");
        let writes = Rc::new(Cell::new(0u64));
        // What the settle's own instrument read at the moment it fired, captured inside
        // `f` before anything writes.
        let at_fire = Rc::new(Cell::new(u64::MAX));
        let second = Rc::new(Cell::new(false));
        arm_settle(&view, FAST, valid, Rc::clone(&writes), {
            let view = view.clone();
            let writes = Rc::clone(&writes);
            let at_fire = Rc::clone(&at_fire);
            let second = Rc::clone(&second);
            move |v| {
                at_fire.set(writes.get());
                let adj = v.vadjustment().expect("still has a vadjustment");
                // Through the seam, like every production write (ScrAP-260) — the
                // point of the write here is that it lands on the adjustment at all.
                crate::saferizer::scrollpos::jump(&adj, adj.value() + 1.0);
                arm_settle(
                    &view,
                    FAST,
                    Rc::new(Cell::new(true)),
                    Rc::new(Cell::new(0)),
                    {
                        let second = Rc::clone(&second);
                        move |_| second.set(true)
                    },
                );
            }
        });
        until_for(
            Clock::Frame,
            Duration::from_secs(5),
            "the re-armed settle to fire",
            {
                let second = Rc::clone(&second);
                move || second.get()
            },
        );
        assert_eq!(
            adjustment.value(),
            1.0,
            "precondition: the restore really did write the adjustment — with no write \
             there is nothing for a live handler to have counted and this proves nothing"
        );
        assert_ne!(at_fire.get(), u64::MAX, "precondition: `f` ran");
        assert_eq!(
            writes.get(),
            at_fire.get(),
            "the settle's own instrument saw nothing after it fired: it was disarmed \
             before `f` wrote, so the count a re-armed wait would inherit is clean"
        );
        window.destroy();
    }

    /// **An unrealized view never runs the deferred work.**
    ///
    /// A zombie retains its last allocation, so no geometry check discriminates —
    /// `is_realized()` is the exact test (GTK4Rs/AP-128). Here the view is realized when
    /// the wait is armed and unrealized before it fires, which is the shape a window
    /// closed mid-settle produces.
    #[gtktest::test]
    fn a_view_unrealized_mid_wait_does_not_run_the_deferred_work() {
        let (view, window, valid) = presented_view();
        let ran = Rc::new(Cell::new(false));
        arm_settle(&view, FAST, valid, Rc::new(Cell::new(0)), {
            let ran = Rc::clone(&ran);
            move |_| ran.set(true)
        });
        window.destroy();
        assert!(
            !view.is_realized(),
            "precondition: the view is a zombie now"
        );
        crate::testpump::drain_for(Clock::Frame, Duration::from_millis(100));
        assert!(
            !ran.get(),
            "the deferred work must not fire against an unrealized view"
        );
    }

    /// **A view whose adjustment never moves completes on a FALSE quiet.**
    ///
    /// With nothing writing the adjustment the write count never moves and every tick
    /// counts as quiet — so with the latch set, the wait completes after
    /// `SETTLE_QUIET_TICKS`. That is the documented degradation rather than a defect,
    /// and it is written down here because the alternative reading ("no writes means no
    /// settle") is the one a reader reaches for.
    ///
    /// **It does NOT reach the `None` branch, and the name used to say it did**
    /// (F-TEST-A-003). `set_vadjustment(None)` on a `GtkTextView` does not leave the
    /// view without one: the widget is a `GtkScrollable` and substitutes a fresh
    /// zero-range `GtkAdjustment`, so `view.vadjustment()` is still `Some` and the
    /// handler is still connected — it simply never fires, which is what produces the
    /// false quiet this actually pins. The genuine `None` branch is unreachable from
    /// `arm_settle`, whose parameter is a `&gtk::TextView`; it is kept as a total answer
    /// rather than an `expect`, for the reason every other total in this module is, and
    /// a future reader should not add a test that claims to exercise it.
    #[gtktest::test]
    fn a_zero_range_adjustment_completes_on_a_false_quiet() {
        let (view, window, valid) = presented_view();
        // Substitutes a fresh zero-range adjustment rather than removing one — see the
        // doc comment. Kept because it is the shortest way to an adjustment nothing
        // writes.
        view.set_vadjustment(None::<&gtk::Adjustment>);
        assert!(
            view.vadjustment().is_some(),
            "precondition, and the correction this test is named for: the view still \
             HAS an adjustment — a zero-range substitute — so what follows is a false \
             quiet and not the absent-adjustment branch"
        );
        let ran = Rc::new(Cell::new(false));
        arm_settle(&view, FAST, valid, Rc::new(Cell::new(0)), {
            let ran = Rc::clone(&ran);
            move |_| ran.set(true)
        });
        until(Clock::Frame, "the settle to fire on a false quiet", {
            let ran = Rc::clone(&ran);
            move || ran.get()
        });
        window.destroy();
    }

    /// **The fixture keeps a real scrollable range, and keeps it DURABLY.**
    ///
    /// A regression guard for the defect that made every other test in this module
    /// unsound on one platform, and it is written as its own test because the failure it
    /// catches is invisible from inside the others: they fail on a *precondition* and
    /// read as defects in the code under test. The fixture used to put a bare
    /// `GtkTextView` in a `GtkWindow`, which is eventually allocated its full natural
    /// height — `page_size` grows to equal `upper`, the range collapses to zero, and
    /// `gtk_adjustment_sanitize_value`'s `CLAMP (value, lower, upper - page_size)`
    /// silently turns every write into 0.
    ///
    /// **The word that matters is DURABLY.** `presented_view` already waits for
    /// `upper > page_size`, and that wait passed — on a transient mid-layout sample that
    /// later resolved away. So this asserts the range twice: once where the wait leaves
    /// it, and again after the loop has been given room to finish laying out. A guard
    /// that only checked the first would have passed on the broken fixture, which is
    /// exactly what the fixture's own wait did.
    ///
    /// MEASURED, macOS 15 / GTK 4.22.4 / Quartz, interleaved and order-balanced at
    /// matched load, 20 trials per arm. **This guard**: 0/20 on the bare arrangement,
    /// 20/20 hosted, failing with `upper 5614, page 5614, view height 5614`. **The settle
    /// test it protects**: 1/20 bare, 20/20 hosted.
    ///
    /// Read those two rows together, because the difference is the argument for this test
    /// existing at all: the guard is **deterministic** where the behaviour it protects is
    /// **flaky**. A defect that shows up in one run in twenty is one a seat can dismiss as
    /// noise — and nearly did. Pinning the *precondition* rather than the behaviour turns
    /// the same defect into a result that cannot be argued with.
    ///
    /// ⚠ **MUTATION-TESTED, AND THIS GUARD IS INERT ON LINUX — it is live only where the
    /// defect is.** Reverting the fixture to the bare arrangement and re-running it here
    /// PASSES: measured `upper` 7218, `page_size` 2322, view height 2322, so a real range
    /// of 4896 survives and there is nothing to catch. On Quartz the same arrangement is
    /// allocated its full layout height and the range is permanently zero.
    ///
    /// **It is NOT the virtual screen clipping the window**, which is the obvious
    /// explanation and was proposed as one. Tested at Xvfb screen heights of 1024, 7000
    /// and 20000 px against a 7218 px document: the view is allocated **2322 px in all
    /// three**, unchanged. So X11 is not truncating a window that would otherwise grow —
    /// the view simply requests a natural height reflecting the layout validated so far,
    /// and never asks for the whole document. That difference is what makes the bare
    /// arrangement survivable here and fatal there, and it means the fixture defect is
    /// genuinely platform-shaped rather than latent-everywhere-and-masked.
    ///
    /// Recorded rather than left to be rediscovered, because a guard that cannot fail on
    /// the canonical platform is indistinguishable from one that is working, and the next
    /// person to "simplify" this will run it on Linux and see green either way. This is
    /// ScrAP-157's shape pointed the other way — there a Linux-era guard was inert on
    /// macOS; here a macOS-found guard is inert on Linux.
    #[gtktest::test]
    fn the_fixture_has_a_scrollable_range_that_survives_layout() {
        let (view, window, _valid) = presented_view();
        let range_of = |view: &gtk::TextView| {
            let a = view
                .vadjustment()
                .expect("the view has a vertical adjustment");
            (a.upper(), a.page_size())
        };
        let (upper, page) = range_of(&view);
        assert!(
            upper > page && page > 0.0,
            "the fixture must start with somewhere to scroll: upper {upper}, page {page}"
        );
        // Let the loop run well past the point the fixture's own wait was satisfied. The
        // broken arrangement passed that wait and collapsed afterwards, so a guard that
        // stopped there would have certified it. `|| false` pumps the whole deadline
        // deliberately — there is no convergence to wait for here, the point is to give
        // layout room to finish and then look again.
        crate::testpump::until_or_for(Clock::Frame, Duration::from_millis(200), || false);
        let (upper, page) = range_of(&view);
        assert!(
            upper > page && page > 0.0,
            "the range collapsed after layout settled — a write to this adjustment now \
             clamps to 0 and every settle test in this module is testing an arrangement \
             the application never builds: upper {upper}, page {page}, view height {}",
            view.height()
        );
        window.destroy();
    }

    /// **A layout that never validates still completes, on the tick cap.**
    ///
    /// The cap is a FAILURE bound, never the completion signal (GTK4Rs/AP-122): the
    /// degradation is a late restore against partial geometry, not an unbounded wait.
    /// Reachable as a test only because the tick is a parameter — at the production
    /// 50 ms it is ten seconds.
    #[gtktest::test]
    fn a_layout_that_never_validates_fires_on_the_tick_cap() {
        let (view, window, _valid) = presented_view();
        let ran = Rc::new(Cell::new(false));
        // The latch left FALSE, which is the one case `settle_should_fire`'s first
        // operand can never satisfy.
        arm_settle(
            &view,
            FAST,
            Rc::new(Cell::new(false)),
            Rc::new(Cell::new(0)),
            {
                let ran = Rc::clone(&ran);
                move |_| ran.set(true)
            },
        );
        until_for(
            Clock::Frame,
            Duration::from_secs(10),
            "the tick cap to fire",
            {
                let ran = Rc::clone(&ran);
                move || ran.get()
            },
        );
        window.destroy();
    }
}
