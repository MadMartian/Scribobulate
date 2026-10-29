//! Programmatic navigation to a marker: the wall-clock [`Deadline`] and budgets that
//! bound it, the [`CardFocus`] intent and [`PendingMarkerOpen`] request it carries
//! across frames, the pure [`group_by_line`] grouping the painter and hit-boxes share,
//! and the view's step / identity lookups and converge-scroll that open a marker's
//! card from the keyboard path and the annotations viewer.

use super::CodePreviewView;
use crate::annotations::Direction;
use gtk::glib;
use gtk::prelude::*;

/// A wall-clock instant to give up at, as a `glib::monotonic_time` (µs) stamp.
///
/// A named type rather than a bare `i64`, because the ONE thing both bounds in this file
/// got wrong was the *shape* of the quantity, not its value: a frame count
/// converts to wall-clock only by assuming a refresh rate the app does not control.
/// Making the shape a type puts it at a choke point — every bound here is constructed by
/// [`Deadline::in_us`] and tested by [`Deadline::expired`], so a future "just count
/// ticks" cannot slip back in unnoticed, and the two sites cannot drift apart again.
///
/// **This is an absolute stamp, deliberately — never an accumulated per-frame delta.**
/// `TabBar::animate_tick` (`widgets/tab/ops.rs`) rightly accumulates a *clamped* frame
/// `dt`, but it is solving a different problem: it eases a position, so a long stall must
/// not jump the ease the whole distance in one step. A deadline has no such need and the
/// clamp would actively harm it — clamping a stall would UNDER-count elapsed time and
/// silently push the give-up past its wall-clock intent, reintroducing exactly the
/// frame-delivery dependence described above. Differencing an absolute clock cannot drift:
/// across a suspend the deadline has simply passed, which is the correct answer.
#[derive(Clone, Copy)]
pub(crate) struct Deadline(i64);

impl Deadline {
    /// A deadline `budget_us` microseconds from now.
    pub(crate) fn in_us(budget_us: i64) -> Self {
        Self(glib::monotonic_time() + budget_us)
    }
    /// Has the wall-clock passed this deadline?
    pub(crate) fn expired(self) -> bool {
        glib::monotonic_time() >= self.0
    }
}

/// Whether opening a comment card should move the keyboard focus into it.
///
/// The two answers belong to two different gestures, and conflating them is what made
/// the annotations viewer unusable from the keyboard: a reader who *clicks a chip* or
/// presses Next/Previous Annotation is asking to READ that comment, so the card takes
/// focus and its Escape / Edit / Remove become reachable. A reader ARROWING DOWN THE
/// VIEWER is browsing, and every row they pass through opened a card that stole the
/// focus out from under the list — so the second arrow press went to the card, which
/// binds no arrows, and the browse ended after exactly one row.
///
/// `Leave` therefore skips **both** halves of the focus move (the view's own
/// `grab_focus` as well as the card's `child_focus`), because either one alone is
/// enough to take focus off the list. The cost is that the card's Escape controller —
/// which only sees a key when a card descendant is focused — cannot dismiss it; the
/// surface that kept the focus owns that key instead (the viewer's own Escape).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CardFocus {
    /// Move focus into the card, as a chip click and the annotation commands do.
    Take,
    /// Leave focus where the caller has it, as the annotations viewer's rows do.
    Leave,
}

/// An armed request to open marker `target`'s popover as soon as its chip is painted,
/// abandoned once `deadline` passes. Held in the view's `pending_marker_open` and fired
/// by `snapshot_layer`.
pub(crate) struct PendingMarkerOpen {
    pub(crate) target: usize,
    /// Whether the navigation that armed this request wants the card to take focus.
    ///
    /// Carried on the request because the open happens on a later frame, in
    /// `snapshot_layer`'s idle dispatch, long after the gesture that decided it has
    /// returned — the intent has nowhere else to live across that gap.
    pub(crate) focus: CardFocus,
    /// Buffer offset of the target's anchor, recorded at arm time.
    ///
    /// Carried on the request rather than re-derived from `markers[target]` at
    /// dispatch so the "has the scroll arrived?" test in `snapshot_layer` costs no
    /// second lookup and cannot go stale if the marker list is rebuilt mid-flight.
    pub(crate) anchor: i32,
    pub(crate) deadline: Deadline,
}

/// Wall-clock budget for one programmatic marker navigation.
///
/// **Wall-clock, not a frame count.** Everything this navigation waits on is measured
/// in TIME — GTK's scroll animation is a fixed 200 ms and is *distance-independent*
/// (`#define ANIMATION_DURATION 200`, gtkscrolledwindow.c:196; armed by the
/// ScrolledWindow, not the view, at :2318) and lazy line validation runs on the clock
/// too. A tick callback, by contrast, fires once per FRAME at a refresh rate the app
/// does not control, so a frame count converts to wall-clock only by assuming a rate.
/// The bound this replaced (`MAX_FRAMES = 45`, "~0.75s at 60Hz") was ~3× the animation
/// at 60Hz but **187 ms at 240Hz — shorter than the 200 ms animation it existed to
/// cover**, so on a high-refresh panel the poll expired before the scroll had even
/// finished and the silent give-up branch fired on every navigation. The same constant
/// fails the opposite way when `gtk-enable-animations` is FALSE (gtkscrolledwindow.c:3601
/// → duration 0 → the value is set instantly): 45 frames is then wildly over-generous.
/// A constant that is both too small and too large depending on the environment is the
/// definition of a wrong-shaped bound; a duration is correct in both.
const NAV_BUDGET_US: i64 = 2_000_000; // 2s — generous; the happy path exits on convergence.

/// Wall-clock budget for the GTK4Rs/AP-118 re-pin guard, replacing a `ticks >= 48` ("~0.8s
/// @60fps") count that inherited exactly the same flaw: at 240Hz it disarmed at
/// ~200 ms, i.e. right at the animation boundary, so a DEFERRED validation scroll
/// landing after that was no longer re-pinned and GTK4Rs/AP-118's original symptom returned
/// (the view jumps to the table top and the click is dropped). Safe to keep armed this
/// long precisely because a genuine user scroll disarms it at once (`user_scrolling`).
pub(super) const REPIN_GUARD_US: i64 = 1_500_000; // 1.5s

/// Group marker indices that share a visual line (identical top-`y`), preserving
/// first-seen order — so multiple annotations on one line collapse to a single
/// margin marker (with a count). Pure; unit-tested without a display.
pub(crate) fn group_by_line(ys: &[i32]) -> Vec<(i32, Vec<usize>)> {
    let mut out: Vec<(i32, Vec<usize>)> = Vec::new();
    for (i, &y) in ys.iter().enumerate() {
        if let Some(g) = out.iter_mut().find(|(gy, _)| *gy == y) {
            g.1.push(i);
        } else {
            out.push((y, vec![i]));
        }
    }
    out
}

impl CodePreviewView {
    /// Step to the annotation one step from buffer offset `from` and open its card —
    /// the production chain `window::annotations_nav::step_annotation` runs, composed
    /// here for tests that hold a bare view with no window to dispatch through.
    ///
    /// It **composes** the real functions rather than reimplementing the walk, so a
    /// test that passes says something about the shipped path; the only thing it leaves
    /// out is the window layer's per-mode presentation, which needs a window to have.
    /// Gated to the integration-test cfg — the same cfg as its only callers, so a plain
    /// `cargo test` does not report it as dead code.
    #[cfg(all(test, feature = "gtk-integration-tests"))]
    pub(crate) fn open_stepped_marker_popover(&self, from: i32, direction: Direction) -> bool {
        let Some(src) = self.marker_src_at_step(from, direction) else {
            return false; // no annotations in this document
        };
        match self.marker_index_for_src(src) {
            Some(index) => self.open_marker_popover_at(index, CardFocus::Take),
            None => false,
        }
    }

    /// Open the comment popover for the marker at `index` in this render's `markers`
    /// (document order). Returns `false` only when `index` is out of range.
    ///
    /// **The single index-addressed navigation primitive.** Both the accessibility
    /// `win.next-annotation` / `win.prev-annotation` path (via
    /// [`open_marker_popover_step`]) and the annotations viewer (via
    /// [`marker_index_for_src`]) route through here, so the two callers exercise ONE
    /// code path and cannot drift. `focus` is the one thing they disagree about, and it
    /// is a parameter rather than a second path for exactly that reason.
    ///
    /// **Two-phase by necessity (GTK4Rs/AP-97).** `marker_hitboxes` is repopulated
    /// on each `snapshot_layer(AboveText)` paint and therefore only ever holds the
    /// **visible** markers — an off-screen target has no hit-box to open against. So:
    /// arm a `pending_marker_open` request, converge-scroll to the anchor
    /// (`converge_and_scroll_to_offset`), and let `snapshot_layer` fire the request the
    /// instant the chip paints. When the target is already on screen the fast path opens
    /// it immediately, so the common case does not flash. The whole navigation is bounded
    /// by `NAV_BUDGET_US` — a **wall-clock** deadline, not a frame count; on expiry the
    /// document is left scrolled to the target rather than silently doing nothing.
    ///
    /// [`open_marker_popover_step`]: Self::open_marker_popover_step
    /// [`marker_index_for_src`]: Self::marker_index_for_src
    pub(crate) fn open_marker_popover_at(&self, index: usize, focus: CardFocus) -> bool {
        use gtk::subclass::prelude::*;
        // A new navigation supersedes any converge-scroll still running from a previous
        // one: bump the token so older tick(s) self-cancel instead of fighting this
        // navigation over the shared vadjustment (see `nav_generation`). Bumped BEFORE the
        // fast path too, so even an immediate on-screen open cancels a prior converge
        // scroll that would otherwise keep pulling the view back to its stale target.
        let generation = self.imp().nav_generation.get().wrapping_add(1);
        self.imp().nav_generation.set(generation);

        let anchor = {
            let markers = self.imp().markers.borrow();
            match markers.get(index) {
                Some(m) => m.anchor,
                None => return false, // out of range (stale render / no such marker)
            }
        };
        // Fast path: already painted this frame ⇒ open without scrolling.
        if let Some((_rect, idxs)) = self.marker_hitbox_containing(index) {
            self.open_marker_popover(&idxs, focus);
            return true;
        }
        // Arm the open-request BEFORE scrolling: `snapshot_layer` fires it the instant the
        // target's hit-box exists (see `pending_marker_open`). This replaces a frame-count
        // poll that re-checked the hit-box every tick and hoped — the request is now an
        // EVENT delivered at exactly the goal state.
        let deadline = Deadline::in_us(NAV_BUDGET_US);
        self.imp()
            .pending_marker_open
            .replace(Some(PendingMarkerOpen {
                target: index,
                focus,
                anchor,
                deadline,
            }));
        self.converge_and_scroll_to_offset(anchor, deadline, generation);
        true
    }

    /// The **identity** (`src_span.start`) of the annotation one step from buffer char
    /// offset `from` in `direction`, wrapping — the inverse lookup of
    /// [`marker_index_for_src`], and the preview's answer to "which annotation is next".
    ///
    /// Answers an identity rather than an index so the window layer can hand it to the
    /// one mode-aware navigator every surface shares, instead of the marker layer having
    /// to know what split mode owes the editor. An index would also be a bet that the
    /// render does not change between choosing and arriving (GTK4Rs/AP-74).
    ///
    /// [`marker_index_for_src`]: Self::marker_index_for_src
    pub(crate) fn marker_src_at_step(&self, from: i32, direction: Direction) -> Option<usize> {
        use gtk::subclass::prelude::*;
        let markers = self.imp().markers.borrow();
        let anchors: Vec<i32> = markers.iter().map(|m| m.anchor).collect();
        let index = crate::annotations::step_index(&anchors, from, direction)?;
        Some(markers.get(index)?.source.construct.captured_at().start)
    }

    /// The index in this render's `markers` of the chip whose annotation begins at
    /// source byte `start` — the annotations viewer's **identity → index** resolver.
    ///
    /// Navigation keys on the annotation's `src_span.start` (its stable identity in the
    /// source), never on a positional row index: the viewer's list is a *filtered
    /// subsequence* of all CriticMarkup constructs, so a row's list position is not its
    /// chip's position (GTK4Rs/AP-74). `None` is the
    /// well-defined "no chip for this annotation" answer — surfaced by the type rather
    /// than by opening a wrong popover — reached when the annotation was filtered out of
    /// `markers` or the render is momentarily stale.
    pub(crate) fn marker_index_for_src(&self, start: usize) -> Option<usize> {
        use gtk::subclass::prelude::*;
        self.imp()
            .markers
            .borrow()
            .iter()
            // Identity is the construct's captured START offset, which is exactly
            // what the viewer's rows key on. `captured_at` (not a live resolve) is
            // right here: both sides derive from the SAME scan of the same source,
            // so they agree by construction — and a resolve would make the lookup
            // depend on the live buffer, which the viewer's key does not.
            .position(|m| m.source.construct.captured_at().start == start)
    }

    /// Scroll to buffer `offset` for a PROGRAMMATIC navigation: converge on the
    /// vadjustment's `upper`, landing with non-animated `set_value`.
    ///
    /// **Why not `scroll_to_mark`** (which `scroll_to_buffer_offset` rightly still uses
    /// for the outline and every user-driven case — ScrAP-14)? For programmatic navigation
    /// to a possibly-fresh, possibly-far target it is the wrong primitive, three ways:
    ///  * It **animates** (the fixed 200 ms above), putting a wait into the budget that
    ///    this path does not need at all.
    ///  * Its pending scroll is a **one-shot** (gtktextview.c:2836/2868) that does not
    ///    re-track as lazy validation changes line heights beneath it, and `flush_scroll`
    ///    validates only ±2× the widget height (:2848-2851) while first-paragraph pinning
    ///    (:4874-4935) holds the view near the top as heights grow. For a fresh + far
    ///    target in a long document it can therefore land near the top and **never**
    ///    converge — the second, independent failure of a frame-count bound, which no
    ///    frame count could fix.
    ///  * Being animated, it **loses to any `set_value`** — including our own GTK4Rs/AP-118
    ///    re-pin guard, since `set_value` calls `end_updating` and cancels an in-flight
    ///    animation (gtkadjustment.c:532). There is one animation slot per adjustment.
    ///
    /// So use the primitive that wins outright, and re-aim it every frame as heights
    /// validate (the GTK4Rs/AP-115 progressive restore). Each tick re-reads `line_yrange` — a
    /// pure cached btree read with NO validation side-effect, which is exactly why it is
    /// safe to call here and exactly why it returns an *estimate* until the line
    /// validates. Re-reading is what converges it.
    ///
    /// Re-aiming every tick does three jobs at once: it drives validation toward the
    /// target, it converges as estimates become real heights, and — because the last aim
    /// simply **stays applied** — it makes the give-up visible: on
    /// timeout the document is left scrolled at the target instead of the old code's
    /// nothing-at-all, where the user pressed the accelerator and saw no change whatever.
    fn converge_and_scroll_to_offset(&self, offset: i32, deadline: Deadline, generation: u64) {
        use gtk::subclass::prelude::*;
        // A programmatic scroll takes over from any hand-scrolling, and caches the target
        // as the re-anchor line (GTK4Rs/AP-14 bookkeeping, matching `scroll_to_buffer_offset`).
        self.imp().user_scrolling.set(false);
        self.imp()
            .restore_target_line
            .set(Some(self.buffer().iter_at_offset(offset).line()));

        // `add_tick_callback` takes an `Fn`, not an `FnMut`, so the convergence state
        // needs interior mutability (Cell — GTK main thread only). The callback's first
        // argument IS the widget, so nothing is captured strongly (GTK4Rs/AP-63: a tick callback
        // strong-capturing its own widget is an uncollectable cycle).
        let last_upper = std::cell::Cell::new(f64::NAN);
        let stable = std::cell::Cell::new(0u32);
        self.add_tick_callback(move |view, _| {
            // Superseded by a NEWER navigation → stop at once, and DON'T touch
            // `pending_marker_open`: that request now belongs to the newer navigation,
            // whose own tick (or `snapshot_layer` dispatch) will service it. Continuing
            // would re-aim the shared vadjustment at this navigation's now-stale target
            // and fight the newer one — the rapid-activation scroll/selection desync
            // (see `nav_generation`).
            if view.imp().nav_generation.get() != generation {
                return glib::ControlFlow::Break;
            }
            // The request is gone — `snapshot_layer` either dispatched it (the chip
            // painted) or it expired. If it was dispatched, `open_marker_popover`'s GTK4Rs/AP-118
            // re-pin guard now owns this adjustment, and our re-aiming would be a SECOND
            // `set_value` writer fighting it over the same slot. Stop.
            if view.imp().pending_marker_open.borrow().is_none() {
                return glib::ControlFlow::Break;
            }
            // Never fight a human: the user grabbing the scrollbar/wheel/keys ends the
            // navigation (`user_scrolling` is set by every hand-scroll input source).
            if view.imp().user_scrolling.get() {
                view.imp().pending_marker_open.replace(None);
                return glib::ControlFlow::Break;
            }
            let Some(vadj) = view.vadjustment() else {
                view.imp().pending_marker_open.replace(None);
                return glib::ControlFlow::Break;
            };
            // Re-aim at the target's current best-known y. yalign 0.0 — top of viewport,
            // matching `scroll_to_buffer_offset`.
            let it = view.buffer().iter_at_offset(offset);
            let (y, _h) = view.line_yrange(&it);
            let max = (vadj.upper() - vadj.page_size()).max(vadj.lower());
            crate::saferizer::scrollpos::jump(&vadj, (y as f64).clamp(vadj.lower(), max));

            let upper = vadj.upper();
            if upper == last_upper.get() {
                stable.set(stable.get() + 1);
            } else {
                stable.set(0);
                last_upper.set(upper);
            }
            // Converged: `upper` stopped growing for 2 consecutive frames, so
            // `line_yrange(target)` is a real height now and the aim just applied is the
            // final, correct one (F4 — reading it BEFORE convergence yields a stale/0 y and
            // silently computes off a wrong origin). Leave the request armed: the paint
            // this scroll causes is what dispatches it.
            if stable.get() >= 2 {
                return glib::ControlFlow::Break;
            }
            if deadline.expired() {
                // Visible give-up. The last aim stays applied, so the document is
                // left scrolled at the target even though the chip never painted. Disarm,
                // so no popover can open at a stale rect later.
                view.imp().pending_marker_open.replace(None);
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
    }
}

#[cfg(test)]
mod marker_tests {
    use super::group_by_line;

    #[test]
    fn distinct_lines_stay_separate_in_order() {
        assert_eq!(
            group_by_line(&[10, 30, 50]),
            vec![(10, vec![0]), (30, vec![1]), (50, vec![2])]
        );
    }

    #[test]
    fn same_line_annotations_collapse_into_one_group() {
        // Three markers on y=30 and one on y=10, interleaved: the y=30 group holds
        // all three (in first-seen order), y=10 holds one.
        assert_eq!(
            group_by_line(&[30, 10, 30, 30]),
            vec![(30, vec![0, 2, 3]), (10, vec![1])]
        );
    }

    #[test]
    fn empty_input_yields_no_groups() {
        assert!(group_by_line(&[]).is_empty());
    }
}
