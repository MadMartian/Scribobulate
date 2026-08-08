//! `farscroll` — issuing a **far** scroll on a `GtkTextView` whose line heights
//! are not yet validated.
//!
//! A `GtkTextView` does not know how tall its document is until it has laid every
//! line out, and it does that lazily, in 2000-pixel chunks, from an idle. Until
//! that finishes, *every* line past the validation frontier reports the same
//! geometry — `line_yrange` gives `y` = the frontier and `h` = 0 — so a scroll
//! aimed anywhere beyond it cannot be computed, and GTK does not compute it: it
//! parks the request and only honours it if the layout happens to be valid
//! already. Nothing re-issues a parked request, so the scroll is simply lost.
//!
//! This module owns the one way the app answers that: [`after_line_heights_validated`]
//! defers work until validation has finished, and [`wire_buffer_ends_scroll`] uses
//! it to make Ctrl+Home / Ctrl+End land on the real ends of a large document.
//!
//! See ScrAP-260 for the two GTK mechanisms involved and the measurements behind
//! the design — including the one that was prototyped and refuted.

use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Run `f` once GTK has finished validating `view`'s line heights.
///
/// **Contract.** `f` runs at most once, on the main loop, at a point where
/// `view`'s layout is fully validated — so `line_yrange`, `line_at_y`, the
/// vadjustment's `upper` and any `scroll_to_*` call report and act on real
/// geometry rather than on the validation frontier. `f` does not run at all if
/// the view has been dropped or unrealized in the meantime.
///
/// **Precondition.** None: on an already-validated view `f` runs on the next main
/// loop turn, so callers need not know whether the view is warm.
///
/// **Why this works** (source-identical in GTK 4.6.9 and 4.22.4 — both trees read;
/// the 4.6–4.12 target sits inside that range).
/// `gtk_text_view_invalidate` installs **two** idles, not one, and both outrank the
/// 200 used here:
/// - `first_validate_callback` at `GTK_PRIORITY_RESIZE - 2` = **108**
///   (gtktextview.c:4846). One-shot — it returns `FALSE` and re-arms by
///   *reinstallation*, its flush having removed the source first (:4760).
/// - `incremental_validate_callback` at `GTK_TEXT_VIEW_PRIORITY_VALIDATE` — a
///   *public* macro, `GDK_PRIORITY_REDRAW + 5` = **125** (gtktextview.h:102,
///   gtktextview.c:4854). This is the one that carries the guarantee: it validates
///   2000px per pass and returns `TRUE`, so the source stays attached *and
///   permanently ready*, flipping to `FALSE` only once `gtk_text_layout_is_valid`
///   (:4808-4826).
///
/// **The two are not interchangeable, and it matters which one carries the guarantee.**
/// On the ordinary path it is the 125 source *alone*: the 108 one dispatches once, early,
/// and is gone. They do different work, too — 108's flush routes to
/// `gtk_text_view_validate_onscreen` (the *onscreen* portion), while 125 grinds the whole
/// document to `gtk_text_layout_is_valid`. The 108 source earns its place only when a
/// *re-invalidation* lands on an already-pending oracle: `gtk_text_view_invalidate`
/// reinstalls it, and `flush_first_validate` removes itself *first* (:4760) precisely so
/// an invalidation arriving mid-flush installs a fresh one.
///
/// Do not summarise this as "both gate the oracle". That reading invites the next reader
/// to notice 108 returns `FALSE`, conclude it is redundant, and be *right on the ordinary
/// path and wrong on the re-invalidation path* — an invariant held by two mechanisms,
/// each of which reads as dead code while the other is intact.
/// GLib dispatches only
/// sources at or above the priority of the first ready one it finds — `check`
/// says so in its own comment, *"never dispatch sources with less priority than
/// the first one we choose to dispatch"* (gmain.c:4104-4105, GLib 2.72.4; the
/// floor is enforced by the `break` at :4017), with no anti-starvation escape —
/// and a `g_idle_add_full` source is unconditionally ready (`g_idle_prepare`
/// /`g_idle_check` both `return TRUE`, gmain.c:5909-5921). So "layout
/// invalid" and "a 125-priority idle is permanently ready" are the same
/// condition, which makes a 200-priority idle a *validation has finished* event —
/// the signal GTK otherwise does not expose (`GtkTextLayout` is private in GTK4).
///
/// **Not an absolute, in two ways that matter.**
/// - *Same `GMainContext` only.* A source on another thread-default context is
///   ordered against nothing here.
/// - *A nested main-loop iteration can slip past it.* `g_main_dispatch` blocks a
///   source for the duration of its own callback unless it set
///   `G_SOURCE_CAN_RECURSE` (gmain.c:3397), which `g_idle_add_full` never sets
///   (gmain.c:5997-6019 — priority and callback only), and both `prepare` and
///   `check` `continue` past blocked sources (gmain.c:3717, :4015) *before* the
///   line that raises the floor (:4104), so a blocked source cannot hold it. So
///   a main-loop iteration pumped from *inside*
///   `incremental_validate_callback`'s call stack — anchored-child allocation is
///   the realistic route in this app — can dispatch this idle with the layout
///   still invalid. That is why the callers below do not merely assume validity:
///   an early fire meets an invalid layout, `scroll_to_mark` queues instead of
///   flushing, nothing moves, and the deadline below picks it up. A silent no-op,
///   never a wrong scroll.
///
/// Note the symmetry, because it is why the deadline is *necessary* rather than
/// belt-and-braces: the guarantee is exactly as strong as "the 125 source stays
/// ready", and that is the same predicate as the starvation risk. The property
/// that makes this idle precise is the property that can starve it.
///
/// The priority is passed explicitly rather than taken from `idle_add_local_once`'s
/// default: it is the entire mechanism, so it is stated where it can be read.
///
/// **Cost of the wait.** `f` runs only when the *whole* document has been laid out,
/// which is seconds on a very large one (measured live: ~18 s for 200 000 lines).
/// That is the price of a correct answer — the document's height is not knowable
/// sooner — so use this for work that would otherwise be *wrong*, not to defer work
/// that merely wants to be late.
///
/// **The bound.** "Permanently ready until valid" is also the failure mode: a layout
/// that never converges starves the idle *by design*, on exactly the documents this
/// exists for. A second source therefore backs it, on a timer rather than an idle,
/// because `g_timeout_add` runs at `G_PRIORITY_DEFAULT` (0) — strictly *above* the
/// validate idle — and so cannot be starved by the thing the idle waits on. It
/// triggers on stalled progress rather than elapsed time, and whichever source runs
/// first wins. So `f` is guaranteed to run: promptly when validation finishes,
/// late and against partial geometry when validation never does. An unbounded hang
/// becomes a bounded degradation.
///
/// **Why `is_realized()` and not `is_mapped()`.** The validate idle is also cleared by
/// `remove_validate_idles`, whose complete set of callers is `unrealize` (:4984),
/// `dispose` (:3917) and `destroy_layout` (:7963) — and `destroy_layout` is reachable
/// only from dispose/finalize, *not* from a buffer swap (`set_buffer` calls
/// `gtk_text_layout_set_buffer` and keeps the layout). So `is_realized()` covers the
/// only case where the widget survives; the rest are teardown, where the weak ref is
/// the guard. `is_mapped()` would be wrong here as well as unnecessary — a realized
/// but unmapped background tab is a legitimate scroll target in this app.
///
/// **But `realized` does not imply "has a viewport".** `_gtk_text_view_scroll_to_iter`
/// computes against `SCREEN_HEIGHT(widget)`, which is 0 on a view that has never been
/// allocated — GTK treats that as a real case (`validate_onscreen` guards
/// `if (SCREEN_HEIGHT (widget) > 0)`, :4718) — and scrolling against a zero viewport
/// lands *wrong*, not nowhere. Scrolling callers must gate on a viewport existing;
/// [`scroll_to_mark_when_ready`] does.
pub(crate) fn after_line_heights_validated<F>(view: &gtk::TextView, f: F)
where
    F: FnOnce(&gtk::TextView) + 'static,
{
    // Weak capture: a strong one would pin the view alive as an unrooted zombie
    // after `window.destroy()` (unrealize is synchronous, finalize is not), and
    // either source below would then fire against it (GTK4Rs/AP-128). `upgrade()` is
    // liveness-only, so the realized state is gated separately.
    let weak = view.downgrade();
    // Whoever gets here first runs it; the loser finds `None` and no-ops. Neither
    // source is ever `.remove()`d — a fired one-shot's `SourceId` must not be
    // (glib 0.21.5 panics on an already-fired id), and letting the loser fire into
    // an empty slot costs one no-op turn.
    let once: Rc<RefCell<Option<F>>> = Rc::new(RefCell::new(Some(f)));
    let run = move |once: &Rc<RefCell<Option<F>>>, weak: &glib::WeakRef<gtk::TextView>| {
        if let (Some(view), Some(f)) = (weak.upgrade(), once.borrow_mut().take()) {
            // A zombie retains its last allocation, so no geometry check can tell it
            // from a live view — `is_realized()` is the exact test (GTK4Rs/AP-128).
            if view.is_realized() {
                f(&view);
            }
        }
    };

    // The precise path: starved until validation finishes (see the doc comment).
    {
        let (once, weak) = (Rc::clone(&once), weak.clone());
        glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
            run(&once, &weak);
            glib::ControlFlow::Break
        });
    }

    // The deadline. `g_timeout_add` runs at `G_PRIORITY_DEFAULT` (0) — strictly
    // ABOVE the validate idle at 125 — so it is immune to precisely what the idle
    // above is vulnerable to: a layout that never becomes valid starves the idle
    // forever, and cannot starve this. That inversion is the whole point; a second
    // idle would inherit the same failure.
    //
    // It fires on lack of PROGRESS, not on elapsed time. A legitimately huge
    // document takes as long as it takes (measured live: ~18 s for 200 000 lines),
    // so a fixed 5-10 s deadline would pre-empt the correct answer and re-create the
    // bug on exactly the documents that need it. The predicate is `upper` CHANGED,
    // deliberately not `upper` GREW: the range does not climb monotonically — a
    // re-wrap or a swap can shrink it (measured elsewhere in this codebase thrashing
    // 3552 → 4288 → 5014 → 2604 → 8719) — and a "grew" test would read a legitimate
    // shrink as a stall and fire mid-validation, landing the re-issue on partial
    // heights, which is the one outcome this whole design exists to avoid. Validation
    // replacing a 0-height line with a real one always CHANGES the total, so only a
    // genuinely stalled validator leaves it bit-identical across a full tick. The
    // absolute cap then bounds a range that oscillates instead of settling.
    //
    // The exact `==` on an `f64` is deliberate, not an oversight to be "modernised"
    // into an epsilon comparison: `upper` is INTEGER-derived — `gtk_text_view_set_vadjustment_values`
    // computes `MAX(screen_height, priv->height)` from two `int`s (`GtkTextViewPrivate`
    // :207-208) and widens the result — so no float arithmetic accumulates on that path
    // and there is no drift for an epsilon to absorb. An epsilon would only lose
    // information, and the granularity it would swallow is exactly the one this bug
    // operates at: GNOME/gtk#2205 is a ONE-PIXEL height nudge cancelling a scroll.
    {
        let mut last_upper = f64::NAN;
        let mut ticks = 0u32;
        glib::timeout_add_local(WATCHDOG_TICK, move || {
            let Some(view) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let upper = view.vadjustment().map(|a| a.upper()).unwrap_or(0.0);
            ticks += 1;
            let fire = watchdog_should_fire(upper, last_upper, ticks);
            last_upper = upper;
            if !fire {
                return glib::ControlFlow::Continue; // still laying out — keep waiting
            }
            run(&once, &weak);
            glib::ControlFlow::Break
        });
    }
}

/// How often the deadline checks whether layout validation is still making
/// progress. Long enough that a healthy document is never judged on one slow tick.
const WATCHDOG_TICK: std::time::Duration = std::time::Duration::from_secs(5);

/// Absolute bound on the deadline, for a layout whose height oscillates rather than
/// settling (which would otherwise extend the progress check forever).
const WATCHDOG_MAX_TICKS: u32 = 12;

/// Whether the deadline should give up waiting on this tick.
///
/// Split out from the timer closure so the two subtleties above are pinned by tests
/// rather than only by prose — both are the kind of thing a later "cleanup" reverses:
///
/// * The predicate is `upper` **CHANGED**, deliberately not `upper` **GREW**. The range
///   does not climb monotonically, so a "grew" test would read a legitimate shrink as a
///   stall and fire mid-validation — landing the re-issue on partial heights, the one
///   outcome this design exists to avoid.
/// * The comparison is an exact `==` on an `f64` **on purpose**, because `upper` is
///   integer-derived and no float arithmetic accumulates on that path. An epsilon would
///   swallow exactly the granularity this bug operates at (a one-pixel nudge).
///
/// The first tick passes `f64::NAN` as `last_upper`, and `NAN == NAN` is false — so a
/// fresh watchdog never reads its own uninitialised state as a stall. That is load-bearing,
/// not incidental, which is why it has a test of its own.
fn watchdog_should_fire(upper: f64, last_upper: f64, ticks: u32) -> bool {
    let stalled = upper == last_upper;
    stalled || ticks >= WATCHDOG_MAX_TICKS
}

/// Whether the caret is still at the end the keystroke asked for.
///
/// The caret *is* the generation token for a pending re-issue (see
/// [`wire_buffer_ends_scroll`]): if the reader has moved since, the re-issue is stale and
/// must be abandoned rather than yanking them back. Split out so that rule is testable
/// without a buffer.
fn caret_still_at_requested_end(to_end: bool, iter_is_end: bool, iter_offset: i32) -> bool {
    if to_end {
        iter_is_end
    } else {
        iter_offset == 0
    }
}

/// Whether `view` has a viewport to scroll *within*.
///
/// A realized view that has never been allocated has a zero-height text window, and
/// GTK scrolls against that height rather than refusing — so the result is a wrong
/// position, not a no-op. `page_size` is this codebase's established proxy for "the
/// scrollable range is real yet" (ScrAP-13); zero means the view is not yet on screen
/// and the caller should leave the position alone.
fn has_viewport(view: &gtk::TextView) -> bool {
    viewport_is_real(view.vadjustment().map(|a| a.page_size()))
}

/// The decision behind [`has_viewport`], over the page size alone.
///
/// Split out so the rule is stated once and testable without a display: a view with
/// no vadjustment at all and one whose adjustment is still a draft (`page_size` 0)
/// are the same answer — not on screen yet, leave the position alone.
fn viewport_is_real(page_size: Option<f64>) -> bool {
    page_size.is_some_and(|p| p > 0.0)
}

/// Make Ctrl+Home / Ctrl+End reach the real ends of a large document in `view`.
///
/// Wire this once per text view, at the view's construction site. It observes
/// `move-cursor` and, for the buffer-ends step only, re-issues the scroll from
/// [`after_line_heights_validated`], so the request is honoured against the
/// document's true height rather than against however much of it GTK had laid
/// out when the key was pressed.
///
/// The re-issue is skipped if the caret is no longer where the keystroke put it
/// — the caret *is* the generation token, so a later navigation (or the opposite
/// key) supersedes an earlier pending re-issue without any bookkeeping, and a
/// stale one can never yank the reader somewhere they have since left.
///
/// GTK's own cursor movement and its own (doomed) scroll attempt are left
/// untouched: on a warm document they already do the right thing and this adds
/// one redundant, identical scroll.
///
/// Wire this on **every** `GtkTextView` the user can put a caret in — both panes.
/// The preview is read-only, but a read-only view still carries GTK's buffer-ends
/// key bindings and still moves its insert mark, so it has the same defect.
pub(crate) fn wire_buffer_ends_scroll(view: &gtk::TextView) {
    // The emitter is the handler's first argument — never captured, so this
    // closure closes no reference cycle over the view (GTK4Rs/AP-63).
    view.connect_move_cursor(|view, step, count, _extend_selection| {
        if step != gtk::MovementStep::BufferEnds {
            // Every other step moves within or beside the viewport, which is
            // validated by definition — only the buffer ends can be beyond the
            // frontier.
            return;
        }
        let to_end = count > 0;
        after_line_heights_validated(view, move |view| {
            let buffer = view.buffer();
            let insert = buffer.get_insert();
            let iter = buffer.iter_at_mark(&insert);
            let still_there = caret_still_at_requested_end(to_end, iter.is_end(), iter.offset());
            if !still_there || !has_viewport(view) {
                return;
            }
            // `scroll_to_mark`, never `scroll_to_iter`: it schedules rather than
            // forcing validation from wherever it is called (GTK4Rs/AP-22). By now the
            // layout is valid, so it takes GTK's own immediate-flush fast path
            // (gtktextview.c:2790-2795) and GTK computes the destination itself.
            #[allow(clippy::disallowed_methods)]
            view.scroll_to_mark(&insert, 0.0, false, 0.0, 0.0);
        });
    });
}

/// Scroll `view` so that `mark` is on screen — now, and again once GTK is able to
/// compute where that actually is.
///
/// **Use this for any navigation to a target the reader is not already looking at**
/// (go to line, a find hit, an outline entry, a link destination). A bare
/// [`TextViewExt::scroll_to_mark`] is banned crate-wide for exactly this reason and
/// `clippy.toml` names this function as the route.
///
/// **Contract.** The scroll is issued immediately, so a laid-out document responds
/// in the same turn and nothing about the warm path changes. If GTK cannot honour
/// it yet — the document is large and still being laid out — the request is
/// *silently discarded* by GTK rather than deferred (ScrAP-260), so it is issued a
/// second time from [`after_line_heights_validated`], when it can be computed
/// correctly.
///
/// **Supersession.** The re-issue is abandoned if the caret has moved since, which
/// is the same "does this navigation still mean what it meant?" test
/// [`wire_buffer_ends_scroll`] uses. Every caller here moves the caret to its
/// target, so a later navigation retires an earlier pending re-issue with no
/// generation bookkeeping — and a re-issue can never drag the reader somewhere they
/// have already left. The target mark is carried through
/// [`BufferMark`](crate::saferizer::buffer_mark::BufferMark), so a `set_buffer`
/// swap in the meantime yields `None` rather than the ScrAP-104 abort.
///
/// The alignment arguments are GTK's own, passed through unchanged.
pub(crate) fn scroll_to_mark_when_ready(
    view: &gtk::TextView,
    mark: &gtk::TextMark,
    within_margin: f64,
    use_align: bool,
    xalign: f64,
    yalign: f64,
) {
    let buffer = view.buffer();
    let caret_at_call = buffer.iter_at_mark(&buffer.get_insert()).offset();
    let target = crate::saferizer::buffer_mark::BufferMark::new(mark.clone(), &buffer);

    // The immediate attempt: correct and sufficient whenever the layout is valid.
    #[allow(clippy::disallowed_methods)]
    view.scroll_to_mark(mark, within_margin, use_align, xalign, yalign);

    after_line_heights_validated(view, move |view| {
        let buffer = view.buffer();
        if buffer.iter_at_mark(&buffer.get_insert()).offset() != caret_at_call {
            return; // superseded — the reader has moved on
        }
        if !has_viewport(view) {
            return; // no viewport to scroll within; see `has_viewport`
        }
        let Some(mark) = target.scroll_mark(&buffer) else {
            return; // the buffer was swapped; a fresh render owns the position now
        };
        #[allow(clippy::disallowed_methods)]
        view.scroll_to_mark(mark, within_margin, use_align, xalign, yalign);
    });
}

#[cfg(test)]
mod decision_tests {
    use super::*;

    // ---- watchdog_should_fire ------------------------------------------------------
    //
    // These pin the two rules the timer closure's prose argues for. Both are reversible
    // by a plausible "cleanup" (== into >, == into an epsilon compare), and neither is
    // observable from the integration tests, which exercise the healthy path where the
    // watchdog never fires at all.

    #[test]
    fn a_fresh_watchdog_does_not_read_its_own_uninitialised_state_as_a_stall() {
        // The first tick compares against NAN, and NAN == NAN is false. If this ever
        // became true the deadline would fire on tick 1 of every wait, defeating the
        // whole design by re-issuing against partial heights immediately.
        assert!(!watchdog_should_fire(1000.0, f64::NAN, 1));
    }

    #[test]
    fn an_unchanged_range_is_a_stall_and_fires() {
        assert!(watchdog_should_fire(4288.0, 4288.0, 3));
    }

    #[test]
    fn a_growing_range_is_progress_and_keeps_waiting() {
        assert!(!watchdog_should_fire(5014.0, 4288.0, 3));
    }

    #[test]
    fn a_shrinking_range_is_progress_too_and_must_not_be_read_as_a_stall() {
        // CHANGED, not GREW. A re-wrap or swap legitimately shrinks the range (measured
        // in this codebase thrashing 3552 -> 4288 -> 5014 -> 2604 -> 8719). A "grew"
        // predicate would call this a stall and fire mid-validation.
        assert!(!watchdog_should_fire(2604.0, 5014.0, 3));
    }

    #[test]
    fn a_one_pixel_change_still_counts_as_progress() {
        // The exact `==` is deliberate: an epsilon comparison would swallow precisely
        // this granularity, and a one-pixel height nudge cancelling a scroll is the
        // upstream defect this module exists for.
        assert!(!watchdog_should_fire(4288.0, 4287.0, 3));
    }

    #[test]
    fn the_tick_cap_fires_even_while_the_range_is_still_changing() {
        // A range that oscillates instead of settling would extend the progress check
        // forever; the absolute cap bounds it.
        assert!(watchdog_should_fire(8719.0, 2604.0, WATCHDOG_MAX_TICKS));
    }

    #[test]
    fn the_tick_cap_is_a_floor_not_an_equality() {
        // Guards against a `==` that a stray extra tick would step straight over.
        assert!(watchdog_should_fire(8719.0, 2604.0, WATCHDOG_MAX_TICKS + 1));
    }

    // ---- viewport_is_real ----------------------------------------------------------

    #[test]
    fn a_view_with_no_adjustment_has_no_viewport() {
        assert!(!viewport_is_real(None));
    }

    #[test]
    fn a_draft_adjustment_with_zero_page_size_has_no_viewport() {
        // ScrAP-13: page_size is this codebase's proxy for "the scrollable range is real
        // yet". Scrolling against a zero viewport lands WRONG, not nowhere.
        assert!(!viewport_is_real(Some(0.0)));
    }

    #[test]
    fn a_positive_page_size_is_a_real_viewport() {
        assert!(viewport_is_real(Some(600.0)));
    }

    // ---- caret_still_at_requested_end ----------------------------------------------

    #[test]
    fn a_caret_left_at_the_buffer_end_still_wants_its_end_scroll() {
        assert!(caret_still_at_requested_end(true, true, 12_345));
    }

    #[test]
    fn a_caret_moved_away_from_the_end_supersedes_the_pending_re_issue() {
        assert!(!caret_still_at_requested_end(true, false, 12_345));
    }

    #[test]
    fn a_caret_left_at_offset_zero_still_wants_its_start_scroll() {
        assert!(caret_still_at_requested_end(false, false, 0));
    }

    #[test]
    fn a_caret_moved_away_from_the_start_supersedes_the_pending_re_issue() {
        assert!(!caret_still_at_requested_end(false, false, 1));
    }

    #[test]
    fn the_two_directions_are_judged_by_different_tests_not_one_shared_one() {
        // A caret at the end of an EMPTY buffer is both is_end() and offset 0, so a
        // single shared predicate would answer the same for opposite requests. This
        // pins that each direction reads its own field.
        assert!(caret_still_at_requested_end(true, true, 0));
        assert!(caret_still_at_requested_end(false, true, 0));
        // ...and that a non-empty end position is NOT accepted as a start position.
        assert!(!caret_still_at_requested_end(false, true, 500));
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;
    use crate::window::build_tab_editor;

    /// Big enough that GTK is still validating line heights when the key is
    /// pressed — the whole precondition of the defect. At 8 000 lines validation
    /// finishes inside the first frames and every assertion here passes with the
    /// fix removed (verified), so a smaller fixture would be a vacuous test.
    const LINES: usize = 20_000;

    /// Where the supersession test parks the caret: far enough down that scrolling
    /// to it is unmistakable in the viewport.
    const FAR_LINE: i32 = 15_000;

    /// A presented editor over a `LINES`-line document, pumped only far enough to
    /// allocate. NOT pumped to a settled layout: a harness that lets validation
    /// finish first removes the precondition and the test passes on broken code
    /// (GTK4Rs/AP-78).
    fn cold_editor() -> (
        sourceview::Buffer,
        sourceview::View,
        gtk::ScrolledWindow,
        gtk::Window,
    ) {
        let body: String = (0..LINES)
            .map(|i| format!("Line {i} — {}\n", "lorem ipsum dolor sit amet ".repeat(4)))
            .collect();
        let (buffer, view) = build_tab_editor(&body);
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_child(Some(&view));
        let window = gtk::Window::new();
        window.set_default_size(800, 600);
        window.set_child(Some(&scroller));
        window.present();
        let ctx = glib::MainContext::default();
        for _ in 0..200 {
            ctx.iteration(false);
            if scroller.vadjustment().page_size() > 0.0 {
                break;
            }
        }
        (buffer, view, scroller, window)
    }

    /// Consecutive turns `upper` must hold one value before the layout counts as
    /// validated. Large enough that a lull between validation chunks is never mistaken
    /// for the end of them.
    const SETTLED_TURNS: u32 = 200;

    /// Pump until GTK has finished validating line heights, judged by the vadjustment's
    /// range CONVERGING rather than by crossing a pixel threshold.
    ///
    /// **This exists because the obvious version is a portability bug.** It was
    /// `pump_until(.., || adjustment.upper() > 600_000.0)`, and that passed only on the
    /// platform it was calibrated on. Measured settled `upper` for this fixture:
    ///
    /// | platform | settled `upper` | `> 600_000`? |
    /// |---|---|---|
    /// | Linux / X11 | 702 018 px | yes |
    /// | Windows, macOS | 540 014 px | **no — unsatisfiable** |
    ///
    /// The constant sat *between* the platforms. Validation completed on all three
    /// (measured: 267 chunks, converged) and the predicate stayed false on two, so the
    /// only reachable outcome there was `pump_until`'s 30 s watchdog — a green test
    /// where it was written and a guaranteed timeout everywhere else. A document's
    /// settled height is a font-metric question, so **no constant can stand in for
    /// "laid out"**; only convergence can. Eight of this module's nine pump sites
    /// already used relative oracles and none of them failed — the one absolute site was
    /// the one that broke, which was visible in the file before anyone measured
    /// anything.
    ///
    /// Two details that are load-bearing rather than incidental:
    /// * The predicate is `upper` **unchanged in VALUE**, not "no longer notifying".
    ///   `upper` emits at least one further *same-value* `notify` after validation
    ///   finishes (measured +8–10 ms, GTK 4.22.4), so an oracle counting notifications
    ///   reports a false "still validating".
    /// * It waits for a change **first**. Otherwise the initial pre-validation value —
    ///   momentarily stable while nothing has been laid out yet — would satisfy a bare
    ///   stability test and report a cold view as settled. `cold_editor` deliberately
    ///   stops pumping the moment a viewport exists (ScrAP-87), so the range really is
    ///   still tiny at that point: 567 px against a settled 540 014.
    ///
    /// Callers take this helper rather than writing a predicate, so a second absolute
    /// threshold cannot reappear one test at a time.
    fn pump_until_layout_validated(adjustment: &gtk::Adjustment) {
        let adjustment = adjustment.clone();
        let mut last = f64::NAN;
        let mut unchanged = 0u32;
        let mut ever_changed = false;
        pump_until("layout validation to finish", move || {
            let upper = adjustment.upper();
            if upper == last {
                unchanged += 1;
            } else {
                // NAN != NAN, so the first turn always lands here and seeds `last`
                // without being counted as a change.
                if !last.is_nan() {
                    ever_changed = true;
                }
                unchanged = 0;
                last = upper;
            }
            ever_changed && unchanged >= SETTLED_TURNS
        });
    }

    /// Pump until `done()`, bounded by a real glib timeout SOURCE — never a
    /// wall-clock check between iterations (GTK4Rs/AP-79). Non-blocking iteration with no
    /// sleep: a sleep between turns throttles the validation idle this test is
    /// waiting on (measured: a 2 ms sleep stretched a 100 ms settle past 3 s).
    fn pump_until(what: &str, mut done: impl FnMut() -> bool) {
        use std::cell::Cell;
        use std::rc::Rc;
        let fired = Rc::new(Cell::new(false));
        let f = Rc::clone(&fired);
        let deadline =
            glib::timeout_add_local_once(std::time::Duration::from_secs(30), move || f.set(true));
        let ctx = glib::MainContext::default();
        let mut deadline = Some(deadline);
        while !done() {
            assert!(
                !fired.get(),
                "pump watchdog (30s) fired waiting for: {what}"
            );
            ctx.iteration(false);
        }
        if let Some(id) = deadline.take() {
            if !fired.get() {
                id.remove();
            }
        }
    }

    /// **Ctrl+End reaches the last line of a document GTK has not finished
    /// laying out.**
    ///
    /// The bottom of the document is `upper - page_size`, and `upper` is not
    /// known until every line height is validated. Pre-fix this asserts the
    /// measured defect: the viewport does not move at all (`value` stays 0) while
    /// `upper` climbs past it, because GTK parked the scroll request and nothing
    /// ever re-issued it.
    #[gtktest::test]
    fn ctrl_end_reaches_the_bottom_of_a_document_still_being_laid_out() {
        let (_buffer, view, scroller, window) = cold_editor();
        let adjustment = scroller.vadjustment();
        assert!(
            adjustment.upper() < 20_000.0,
            "precondition: the layout must still be unvalidated when the key is \
             pressed — upper was already {:.0}, so this fixture no longer exercises \
             the defect (GTK4Rs/AP-78)",
            adjustment.upper()
        );

        view.emit_move_cursor(gtk::MovementStep::BufferEnds, 1, false);

        let bottom = || (adjustment.upper() - adjustment.page_size()).max(0.0);
        pump_until("the viewport to reach the bottom", || {
            adjustment.value() >= bottom() - 1.0
        });

        assert!(
            adjustment.value() >= bottom() - 1.0,
            "Ctrl+End must land on the last line: value {:.0}, bottom {:.0} \
             (upper {:.0}, page {:.0})",
            adjustment.value(),
            bottom(),
            adjustment.upper(),
            adjustment.page_size()
        );
        window.destroy();
    }

    /// **Ctrl+Home reaches the first line of a document GTK has not finished
    /// laying out.**
    ///
    /// The top needs no geometry at all — it is 0 — yet pre-fix this also fails,
    /// by the *other* GTK mechanism: `scroll_to_mark` scrolls by animating the
    /// adjustment, and the validation idle's own `set_value` re-anchor cancels a
    /// running animation, so the viewport stops a fraction of the way up
    /// (measured 30 000 → 23 107 of a target 0) and stays there.
    #[gtktest::test]
    fn ctrl_home_reaches_the_top_of_a_document_still_being_laid_out() {
        let (_buffer, view, scroller, window) = cold_editor();
        let adjustment = scroller.vadjustment();
        pump_until("a scrollable range to exist", || {
            adjustment.upper() - adjustment.page_size() > 30_000.0
        });
        crate::saferizer::scrollpos::jump(&adjustment, 30_000.0);

        view.emit_move_cursor(gtk::MovementStep::BufferEnds, -1, false);

        pump_until("the viewport to reach the top", || {
            adjustment.value() <= 0.0
        });
        assert_eq!(
            adjustment.value(),
            0.0,
            "Ctrl+Home must land on the first line, not part of the way there"
        );
        window.destroy();
    }

    /// **A far navigation reaches its target on a document still being laid out.**
    ///
    /// This is Go To Line / a find hit / an outline entry, not the buffer ends —
    /// the same root cause with an interior target. Pre-fix, a cold 40 000-line
    /// document sent Go To Line 30 000 to line 177 and left it there.
    #[gtktest::test]
    fn a_far_navigation_reaches_its_target_on_a_document_still_being_laid_out() {
        let (buffer, view, scroller, window) = cold_editor();
        let adjustment = scroller.vadjustment();
        assert!(
            adjustment.upper() < 20_000.0,
            "precondition: the layout must still be unvalidated (GTK4Rs/AP-78)"
        );

        let target = buffer
            .iter_at_line(FAR_LINE)
            .expect("the fixture is that long");
        buffer.place_cursor(&target);
        let mark = buffer.create_mark(None, &target, true);
        scroll_to_mark_when_ready(view.upcast_ref(), &mark, 0.0, true, 0.0, 0.5);

        pump_until("the viewport to reach the target line", || {
            let (top, _) = view.line_at_y(adjustment.value() as i32);
            (top.line() - FAR_LINE).abs() < 40
        });

        let (top, _) = view.line_at_y(adjustment.value() as i32);
        assert!(
            (top.line() - FAR_LINE).abs() < 40,
            "a navigation to line {FAR_LINE} must arrive there, not stop where the \
             layout happened to have got to — the viewport is showing line {}",
            top.line()
        );
        window.destroy();
    }

    /// **The read-only preview pane has the same keys and the same defect.**
    ///
    /// Read-only does not exempt a `GtkTextView`: GTK's buffer-ends bindings live on
    /// the view, not on editability. Wiring one pane and not the other is exactly
    /// the surface-shaped gap ScrAP-234 records.
    #[gtktest::test]
    fn ctrl_end_reaches_the_bottom_of_the_preview_pane_too() {
        let view = crate::codeview::CodePreviewView::new();
        let body: String = (0..LINES)
            .map(|i| format!("Line {i} — {}\n", "lorem ipsum dolor sit amet ".repeat(4)))
            .collect();
        view.buffer().set_text(&body);
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_child(Some(&view));
        let window = gtk::Window::new();
        window.set_default_size(800, 600);
        window.set_child(Some(&scroller));
        window.present();
        let ctx = glib::MainContext::default();
        for _ in 0..200 {
            ctx.iteration(false);
            if scroller.vadjustment().page_size() > 0.0 {
                break;
            }
        }
        let adjustment = scroller.vadjustment();

        view.emit_move_cursor(gtk::MovementStep::BufferEnds, 1, false);

        let bottom = || (adjustment.upper() - adjustment.page_size()).max(0.0);
        pump_until("the preview viewport to reach the bottom", || {
            adjustment.value() >= bottom() - 1.0
        });
        assert!(
            adjustment.value() >= bottom() - 1.0,
            "Ctrl+End in the preview must land on the last line: value {:.0}, bottom {:.0}",
            adjustment.value(),
            bottom()
        );
        window.destroy();
    }

    /// **A queued scroll survives an adjustment write into its own view.**
    ///
    /// `gtk_text_view_value_changed` **destroys** `first_validate_idle`
    /// (gtktextview.c:8437-8443), the only thing that ever consumes a `GtkTextView`'s
    /// pending scroll — so any write to a view's adjustment orphans a scroll *that
    /// view* had queued. Per-view, which is the detail that decides how to aim this:
    /// an earlier version of this test wrote into a second view over the same buffer
    /// and therefore exercised nothing.
    ///
    /// This app writes adjustments from split-pane sync, reading-position restore and
    /// outline navigation, so the collision is reachable in ordinary use. It passes
    /// because the re-issue does not depend on GTK's pending scroll surviving — the
    /// property that makes the fix robust to every adjustment write in the app, and
    /// the reason it sidesteps GNOME/gtk#7507 and #2205 rather than working around them.
    /// [`gtk_own_pending_scroll_is_destroyed_by_a_write_to_its_view`] is the control
    /// showing GTK alone does not survive it.
    #[gtktest::test]
    fn a_queued_scroll_survives_a_write_into_its_own_view() {
        let (_buffer, view, scroller, window) = cold_editor();
        let adjustment = scroller.vadjustment();

        view.emit_move_cursor(gtk::MovementStep::BufferEnds, 1, false);
        // Land the write while the scroll is still pending — the collision itself.
        for _ in 0..30 {
            glib::MainContext::default().iteration(false);
        }
        crate::saferizer::scrollpos::jump(&adjustment, 250.0);

        let bottom = || (adjustment.upper() - adjustment.page_size()).max(0.0);
        pump_until("the scroll to arrive despite the write", || {
            adjustment.value() >= bottom() - 1.0
        });
        assert!(
            adjustment.value() >= bottom() - 1.0,
            "an adjustment write must not orphan this view's own queued Ctrl+End: \
             value {:.0} of a bottom {:.0}",
            adjustment.value(),
            bottom()
        );
        window.destroy();
    }

    /// **Control: GTK's own pending scroll does NOT survive that write.**
    ///
    /// A bare `GtkTextView` — no wiring from this module — on a **fully laid out**
    /// document, so that GTK's own `scroll_to_mark` arrives unaided and the write is
    /// the only variable. A cold view is the wrong fixture here: it fails to arrive
    /// for the *other* reason (the request is never flushed at all), so the write
    /// would evidence nothing. Warm, the scroll takes GTK's immediate-flush path and
    /// what the write destroys is the ~200 ms animation carrying it — GNOME/gtk#2205.
    ///
    /// Without the write the view reaches the end; with it, it does not. That is the
    /// characterisation of the GTK defect itself, and the evidence that applications
    /// lose scrolls through their own ordinary adjustment writes and not merely
    /// through GTK's internal ones.
    #[gtktest::test]
    fn gtk_own_pending_scroll_is_destroyed_by_a_write_to_its_view() {
        /// Big enough that the end is far off screen (so arriving is a real event),
        /// small enough to lay out fully in the settle below.
        const SMALL: usize = 4_000;

        let build = || {
            let view = gtk::TextView::new();
            view.set_wrap_mode(gtk::WrapMode::Word);
            let body: String = (0..SMALL)
                .map(|i| format!("Line {i} — {}\n", "lorem ipsum dolor sit amet ".repeat(4)))
                .collect();
            view.buffer().set_text(&body);
            let scroller = gtk::ScrolledWindow::new();
            scroller.set_child(Some(&view));
            let window = gtk::Window::new();
            window.set_default_size(800, 600);
            window.set_child(Some(&scroller));
            window.present();
            // Pump until layout validation has FINISHED — the same starved-idle
            // signal this module is built on, used here to establish the fixture.
            let done = std::rc::Rc::new(std::cell::Cell::new(false));
            let d = std::rc::Rc::clone(&done);
            glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
                d.set(true);
                glib::ControlFlow::Break
            });
            let ctx = glib::MainContext::default();
            for _ in 0..20000 {
                ctx.iteration(false);
                if done.get() {
                    break;
                }
            }
            (view, scroller, window)
        };
        // The scroll is carried by a ~200 ms frame-clock ANIMATION, so this has to
        // pass real time, not just turns: `iteration(false)` in a tight loop never
        // advances the frame clock's timer source and the animation cannot progress.
        let settle = |adjustment: &gtk::Adjustment| {
            let ctx = glib::MainContext::default();
            let t0 = std::time::Instant::now();
            while t0.elapsed() < std::time::Duration::from_millis(1200) {
                ctx.iteration(false);
                std::thread::sleep(std::time::Duration::from_millis(4));
            }
            adjustment.value() >= (adjustment.upper() - adjustment.page_size()).max(0.0) - 1.0
        };

        // Positive control: unaided, it arrives. Without this the test could pass
        // because nothing ever worked, which would evidence nothing (ScrAP-217).
        let (view, scroller, window) = build();
        view.emit_move_cursor(gtk::MovementStep::BufferEnds, 1, false);
        let arrived_unaided = settle(&scroller.vadjustment());
        window.destroy();
        assert!(
            arrived_unaided,
            "precondition: at {SMALL} lines a bare GtkTextView must reach the end on \
             its own, or this fixture cannot isolate the write's effect"
        );

        // Same again, with one adjustment write landing while the scroll is pending.
        let (view, scroller, window) = build();
        view.emit_move_cursor(gtk::MovementStep::BufferEnds, 1, false);
        for _ in 0..30 {
            glib::MainContext::default().iteration(false);
        }
        crate::saferizer::scrollpos::jump(&scroller.vadjustment(), 250.0);
        let arrived_after_write = settle(&scroller.vadjustment());
        let (value, bottom) = (
            scroller.vadjustment().value(),
            scroller.vadjustment().upper() - scroller.vadjustment().page_size(),
        );
        window.destroy();

        assert!(
            !arrived_after_write,
            "GTK's own pending scroll is expected to be DESTROYED by a write to its \
             view (value {value:.0} of {bottom:.0}). If this now arrives, GTK has \
             fixed #7507/#2205 — verify against the current GTK and, if so, this \
             module's re-issue may be reducible. Do not simply delete this test."
        );
    }

    /// **A pending re-issue never yanks a reader who has since navigated away.**
    ///
    /// The deferred scroll can sit for seconds on a large document. Its licence to
    /// act is the caret still being where the keystroke put it, so moving the
    /// caret in the meantime must silently retire it — otherwise the re-issue
    /// would drag the reader off to wherever the caret has since gone, seconds
    /// after they stopped asking for it.
    ///
    /// The caret is moved somewhere FAR, not somewhere near: the two outcomes have
    /// to be distinguishable. Parked near the top, "the gate held" and "the gate
    /// was skipped and scrolled to the caret" produce the same viewport, and the
    /// test passes with the gate deleted — which is what an earlier version of it
    /// did.
    #[gtktest::test]
    fn a_superseded_buffer_ends_scroll_is_abandoned() {
        let (buffer, view, scroller, window) = cold_editor();
        let adjustment = scroller.vadjustment();

        view.emit_move_cursor(gtk::MovementStep::BufferEnds, 1, false);
        // Same turn: the caret goes somewhere far away before the deferred
        // re-issue can run.
        let elsewhere = buffer
            .iter_at_line(FAR_LINE)
            .expect("the fixture has that many lines");
        buffer.place_cursor(&elsewhere);

        pump_until_layout_validated(&adjustment);
        // Further settled turns, so a re-issue would have had every chance.
        pump_until("the layout to settle", {
            let mut turns = 0;
            move || {
                turns += 1;
                turns > 200
            }
        });

        let (top, _) = view.line_at_y(adjustment.value() as i32);
        assert!(
            top.line() < 100,
            "a Ctrl+End whose caret has moved on must scroll nowhere at all — the \
             viewport is showing line {} (value {:.0}), and line {FAR_LINE} is where \
             the caret went, so the deferred scroll ran when it had no licence to",
            top.line(),
            adjustment.value()
        );
        window.destroy();
    }
}
