//! The complete-click seam: a click activates only when its press AND its release
//! landed on the same target — and, where the caller asks, only when the pointer did
//! not drag its way between the two.

use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Where a press landed, carried until its release so the two can be compared.
struct Press<T> {
    /// The identity of the thing the press hit (a link's span+URL, a checkbox index,
    /// a marker group). Compared — not merely tested for presence — so a press and a
    /// release on two *different* targets cannot pair up.
    target: T,
    x: f64,
    y: f64,
}

/// Wires a `GtkGestureClick` so its activation callback can fire only for a
/// **complete click**: one whose press and release both landed on the same target.
///
/// # Contract
///
/// `GtkGestureClick` reports `pressed` and `released` as two independent signals and
/// pairs them for you in neither direction: a handler on `released` alone fires for
/// *any* release over the target, including the release that ends a drag which began
/// somewhere else entirely. In a `GtkTextView` that is not a corner case but the most
/// ordinary gesture there is — swipe-selecting a passage of text ends wherever the
/// pointer stops, and if that is over a link, a release-only handler opens it. The
/// user pressed to select; the app navigated away.
///
/// GTK's own widgets do not behave that way, because they do this pairing internally:
/// `gtk_label_click_gesture_pressed` sets `link_clicked` only when the press landed on
/// a link, and `gtk_label_click_gesture_released` activates only if that flag is set —
/// *and* the label has no selection, so a drag that selected text is not a click
/// either (`gtklabel.c`, 4.6.9). A hand-wired gesture gets none of that; it must
/// reproduce it, and every call site that reproduces it by hand is a call site that
/// can forget to (ScrAP-238).
///
/// So this type owns **both** connections. The press half is not something a caller
/// can decline to write, because the caller never writes either half —
/// [`wire`](Self::wire) takes a hit-test and an activation and connects the signals
/// itself. `GestureClick::connect_released` is banned crate-wide
/// (`clippy.toml`) so the release-only shape cannot be re-introduced.
///
/// Three options, all defaulting to the strict reading:
///
/// * [`claim_on_press`](Self::claim_on_press) — claim the event sequence when the
///   press hits, so an owning widget's own gesture (a `GtkTextView`'s selection drag)
///   never sees it. Off by default: claiming is a decision about who owns the
///   sequence, not about what completes a click.
/// * [`release_slop`](Self::release_slop) — a pixel tolerance letting a release that
///   drifted just off the target still count, provided it stayed within that distance
///   of the press point. Zero by default (exact same target required). Use it only
///   where the target is small enough that sub-pixel drift between press and release
///   loses otherwise-valid clicks.
/// * [`max_travel`](Self::max_travel) — an upper bound on how far the pointer may
///   move between press and release, so a *drag* that begins and ends on one target
///   still is not a click. Unbounded by default. A target big enough to swipe across
///   (a link caption, a paragraph-long span) wants it: press and release on the same
///   link are also what selecting that link's text looks like, and the reader who
///   selected a caption to copy it did not ask to navigate. GTK draws the same
///   distinction with the same shape — `gtk_label_click_gesture_released` requires
///   `selection_anchor == selection_end` next to its pressed-on-a-link flag
///   (`gtklabel.c`, 4.6.9). Travel rather than the buffer's selection state because a
///   press *inside* an existing selection is deliberately not cleared until the drag
///   ends (`gtktextview.c`), so "is something selected right now" answers a different
///   question at release time than the one being asked.
#[derive(Clone, Copy)]
pub(crate) struct ClickActivation {
    claim_on_press: bool,
    release_slop: f64,
    max_travel: Option<f64>,
}

impl ClickActivation {
    /// The strict form: activation requires a release on the same target the press
    /// hit, the sequence is left unclaimed, and no drift is tolerated.
    pub(crate) fn new() -> Self {
        Self {
            claim_on_press: false,
            release_slop: 0.0,
            max_travel: None,
        }
    }

    /// Claim the event sequence on a press that hits a target, so the owning widget's
    /// own gesture never starts a competing interaction (text selection) from it.
    pub(crate) fn claim_on_press(mut self) -> Self {
        self.claim_on_press = true;
        self
    }

    /// Accept a release that drifted off the target by at most `px` from the press
    /// point as still completing the click.
    pub(crate) fn release_slop(mut self, px: f64) -> Self {
        self.release_slop = px;
        self
    }

    /// Refuse to activate when the pointer travelled more than `px` between press and
    /// release — the press and release may be on the same target and still be the two
    /// ends of a drag, not a click.
    ///
    /// [`drag_threshold`] is the value to pass unless there is a reason not to: it is
    /// the desktop's own click-versus-drag boundary, so this seam agrees with every
    /// other widget the user has ever dragged.
    pub(crate) fn max_travel(mut self, px: f64) -> Self {
        self.max_travel = Some(px);
        self
    }

    /// Connect `gesture`'s press and release to `hit` and `activate`.
    ///
    /// `hit` answers "what is under these widget coordinates" (`None` for nothing);
    /// `activate` runs only for a complete click, and receives the gesture (to claim
    /// the sequence), the target the **press** identified, and the release point.
    pub(crate) fn wire<T, H, A>(self, gesture: &gtk::GestureClick, hit: H, activate: A)
    where
        T: PartialEq + 'static,
        H: Fn(f64, f64) -> Option<T> + 'static,
        A: Fn(&gtk::GestureClick, T, f64, f64) + 'static,
    {
        let hit = Rc::new(hit);
        let tracker = Rc::new(RefCell::new(ClickTracker::new(self)));

        let hit_p = Rc::clone(&hit);
        let tracker_p = Rc::clone(&tracker);
        gesture.connect_pressed(move |gesture, _, x, y| {
            if tracker_p.borrow_mut().press(hit_p(x, y), x, y) {
                gesture.set_state(gtk::EventSequenceState::Claimed);
            }
        });

        let tracker_c = Rc::clone(&tracker);
        gesture.connect_cancel(move |_, _| tracker_c.borrow_mut().cancel());

        let tracker_r = Rc::clone(&tracker);
        // The sole sanctioned `connect_released`: it is banned everywhere else
        // precisely so that this pairing cannot be bypassed (see the type docs).
        #[allow(clippy::disallowed_methods)]
        gesture.connect_released(move |gesture, _, x, y| {
            // One statement, so the tracker's borrow is released before `activate`
            // runs: an activation can rebuild the widget tree and re-enter (ScrAP-53).
            let activated = tracker_r.borrow_mut().release(hit(x, y).as_ref(), x, y);
            if let Some(target) = activated {
                activate(gesture, target, x, y);
            }
        });
    }
}

/// The press/release pairing itself: every decision the wired gesture makes, as a
/// plain state machine over plain data with no GTK object in it.
///
/// Separate from the wiring on purpose. What is worth testing here is the *sequence* —
/// a press that missed erasing the one before it, a cancel dropping a pending press,
/// a release pairing only with the press it belongs to — and none of that needs a
/// display, a widget, or an event. [`ClickActivation::wire`] is then three closures
/// that do nothing but forward, which is the part no test can reach and the part with
/// no decisions left in it.
struct ClickTracker<T> {
    opts: ClickActivation,
    pressed: Option<Press<T>>,
}

impl<T: PartialEq> ClickTracker<T> {
    fn new(opts: ClickActivation) -> Self {
        Self {
            opts,
            pressed: None,
        }
    }

    /// Record a press on `target` (`None` = it hit nothing). Returns whether the caller
    /// should claim the event sequence.
    ///
    /// The pending press is overwritten unconditionally, including with `None`: a press
    /// that missed must ERASE the one before it, or a later release could complete a
    /// click whose press is long gone.
    fn press(&mut self, target: Option<T>, x: f64, y: f64) -> bool {
        let claim = self.opts.claim_on_press && target.is_some();
        self.pressed = target.map(|target| Press { target, x, y });
        claim
    }

    /// Drop the pending press. A cancelled sequence never reaches `released`, so a press
    /// left behind could only ever pair with some later, unrelated release.
    fn cancel(&mut self) {
        self.pressed = None;
    }

    /// Consume the pending press and report the target to activate, if the release at
    /// `(x, y)` over `released` completes the click that press began.
    fn release(&mut self, released: Option<&T>, x: f64, y: f64) -> Option<T> {
        let press = self.pressed.take()?;
        let travel = |px: f64, py: f64| ((x - px).abs(), (y - py).abs());
        let (dx, dy) = travel(press.x, press.y);
        let within = |limit: f64| dx <= limit && dy <= limit;
        // Landed on the target the press identified — or close enough to the press
        // point that a hair's drift off a small target should not lose the click.
        let landed = released == Some(&press.target)
            || (self.opts.release_slop > 0.0 && within(self.opts.release_slop));
        // …and got there without dragging, where the caller says that matters.
        let completes = landed && self.opts.max_travel.is_none_or(within);
        completes.then_some(press.target)
    }
}

/// The desktop's own click-versus-drag boundary (`gtk-dnd-drag-threshold`), the value
/// to hand [`ClickActivation::max_travel`] so an activation agrees with every other
/// drag the user performs.
///
/// **Call it after GTK is initialised** — `gtk::Settings::default()` asserts on that,
/// which every caller satisfies by construction (they are wiring a widget). The
/// `None` arm covers a settings-less display only, and answers with GTK's own default
/// for the setting rather than a zero, which would make every click read as a drag.
pub(crate) fn drag_threshold() -> f64 {
    /// `gtk-dnd-drag-threshold`'s documented default, for the no-settings case.
    const GTK_DEFAULT_DRAG_THRESHOLD: i32 = 8;
    gtk::Settings::default()
        .map_or(GTK_DEFAULT_DRAG_THRESHOLD, |s| s.gtk_dnd_drag_threshold())
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tracker over string targets, driven the way the wired gesture drives it.
    fn tracker(opts: ClickActivation) -> ClickTracker<String> {
        ClickTracker::new(opts)
    }

    fn t(name: &str) -> String {
        name.to_string()
    }

    /// The reported defect, as the sequence that produces it: a swipe that began on
    /// nothing — or on a *different* link — and ended over a link.
    #[test]
    fn a_release_that_does_not_match_its_press_never_activates() {
        let mut tr = tracker(ClickActivation::new());

        tr.press(None, 150.0, 10.0);
        assert_eq!(
            tr.release(Some(&t("link-a")), 10.0, 10.0),
            None,
            "a release over a link whose press landed on nothing must not activate"
        );

        tr.press(Some(t("link-a")), 10.0, 10.0);
        assert_eq!(
            tr.release(Some(&t("link-b")), 300.0, 300.0),
            None,
            "a release over a DIFFERENT target must not pair with the press"
        );

        tr.press(Some(t("link-a")), 10.0, 10.0);
        assert_eq!(
            tr.release(None, 300.0, 300.0),
            None,
            "a press dragged off its target onto nothing must not activate"
        );
    }

    #[test]
    fn a_release_on_the_pressed_target_activates_it() {
        let mut tr = tracker(ClickActivation::new());
        tr.press(Some(t("link-a")), 10.0, 10.0);
        assert_eq!(
            tr.release(Some(&t("link-a")), 40.0, 12.0),
            Some(t("link-a")),
            "press and release on the same target is exactly what activates"
        );
    }

    /// A press that hits nothing must ERASE the press before it — otherwise a release
    /// could complete a click whose press belonged to an earlier, abandoned gesture.
    #[test]
    fn a_press_that_misses_erases_the_pending_press() {
        let mut tr = tracker(ClickActivation::new());
        tr.press(Some(t("link-a")), 10.0, 10.0);
        tr.press(None, 500.0, 500.0);
        assert_eq!(
            tr.release(Some(&t("link-a")), 10.0, 10.0),
            None,
            "the stale press must not survive a press that hit nothing"
        );
    }

    /// A release with no press behind it at all (the sequence began elsewhere, or its
    /// press was already consumed) activates nothing.
    #[test]
    fn a_release_without_a_press_activates_nothing() {
        let mut tr = tracker(ClickActivation::new());
        assert_eq!(tr.release(Some(&t("link-a")), 10.0, 10.0), None);

        tr.press(Some(t("link-a")), 10.0, 10.0);
        assert!(tr.release(Some(&t("link-a")), 10.0, 10.0).is_some());
        assert_eq!(
            tr.release(Some(&t("link-a")), 10.0, 10.0),
            None,
            "one press completes at most one click"
        );
    }

    /// GTK cancels a sequence another gesture claims, and then no release ever arrives
    /// for it — so the pending press must go, not wait for the next release.
    #[test]
    fn a_cancelled_sequence_drops_its_press() {
        let mut tr = tracker(ClickActivation::new());
        tr.press(Some(t("link-a")), 10.0, 10.0);
        tr.cancel();
        assert_eq!(
            tr.release(Some(&t("link-a")), 10.0, 10.0),
            None,
            "a cancelled press must not complete on a later release"
        );
    }

    /// Claiming is opt-in, and only for a press that actually hit something — claiming
    /// a press that hit nothing would take the sequence away from text selection.
    #[test]
    fn the_sequence_is_claimed_only_when_asked_and_only_on_a_hit() {
        let mut plain = tracker(ClickActivation::new());
        assert!(
            !plain.press(Some(t("box-1")), 10.0, 10.0),
            "by default a gesture never claims"
        );

        let mut claiming = tracker(ClickActivation::new().claim_on_press());
        assert!(
            claiming.press(Some(t("box-1")), 10.0, 10.0),
            "claim_on_press claims a press that hit a target"
        );
        assert!(
            !claiming.press(None, 10.0, 10.0),
            "a press that hit nothing is not the gesture's to claim"
        );
    }

    /// Slop rescues a release that drifted off the target, and only within its radius
    /// — it is a jitter allowance, never a way for a far-away release to complete.
    #[test]
    fn slop_admits_only_a_release_near_the_press_point() {
        let mut tr = tracker(ClickActivation::new().release_slop(8.0));
        tr.press(Some(t("box-3")), 100.0, 100.0);
        assert_eq!(
            tr.release(None, 104.0, 96.0),
            Some(t("box-3")),
            "a release within the slop radius still completes"
        );

        tr.press(Some(t("box-3")), 100.0, 100.0);
        assert_eq!(
            tr.release(None, 100.0, 130.0),
            None,
            "a release outside the slop radius does not"
        );

        let mut strict = tracker(ClickActivation::new());
        strict.press(Some(t("box-3")), 100.0, 100.0);
        assert_eq!(
            strict.release(None, 104.0, 96.0),
            None,
            "with no slop configured, only the target itself completes"
        );
    }

    /// A swipe-selection that begins and ends inside ONE link is still a drag: the
    /// same-target rule alone cannot see it, which is what `max_travel` is for.
    #[test]
    fn travel_beyond_the_bound_is_a_drag_not_a_click() {
        let mut bounded = tracker(ClickActivation::new().max_travel(8.0));
        bounded.press(Some(t("link-a")), 100.0, 100.0);
        assert_eq!(
            bounded.release(Some(&t("link-a")), 105.0, 101.0),
            Some(t("link-a")),
            "a press and release on one target, barely moved, is a click"
        );

        bounded.press(Some(t("link-a")), 100.0, 100.0);
        assert_eq!(
            bounded.release(Some(&t("link-a")), 260.0, 100.0),
            None,
            "a swipe across the SAME target is a drag, and must not activate"
        );

        let mut unbounded = tracker(ClickActivation::new());
        unbounded.press(Some(t("link-a")), 100.0, 100.0);
        assert_eq!(
            unbounded.release(Some(&t("link-a")), 260.0, 100.0),
            Some(t("link-a")),
            "unbounded travel is the default — only a caller that asks gets the bound"
        );
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;
    use std::cell::Cell;

    /// Drive the wired gesture through its own `pressed`/`released` signals and assert
    /// the pairing end-to-end — the wiring, not just `completes`. Emitting the signals
    /// directly is what makes this headless: a real press/release pair would need a
    /// mapped window, a pointer device and a text view laid out to known coordinates,
    /// none of which the rule under test depends on.
    ///
    /// Targets are keyed off x: `x < 100` is target 0, `x >= 200` is target 1, the
    /// band between them is nothing at all.
    #[gtktest::test]
    fn activation_requires_press_and_release_on_the_same_target() {
        fn target_at(x: f64, _y: f64) -> Option<u8> {
            match x {
                x if x < 100.0 => Some(0),
                x if x >= 200.0 => Some(1),
                _ => None,
            }
        }
        let fired: Rc<Cell<u32>> = Rc::new(Cell::new(0));
        let gesture = gtk::GestureClick::new();
        let seen = Rc::clone(&fired);
        ClickActivation::new().wire(&gesture, target_at, move |_, _target: u8, _, _| {
            seen.set(seen.get() + 1)
        });

        let press = |x: f64| gesture.emit_by_name::<()>("pressed", &[&1i32, &x, &0.0f64]);
        let release = |x: f64| gesture.emit_by_name::<()>("released", &[&1i32, &x, &0.0f64]);

        // The reported bug: a drag that began on ordinary text (no target) and ended
        // over one.
        press(150.0);
        release(50.0);
        assert_eq!(fired.get(), 0, "a release-only click must not activate");

        // Press on one target, release on another.
        press(50.0);
        release(250.0);
        assert_eq!(
            fired.get(),
            0,
            "a press and release on two targets must not pair"
        );

        // Press on a target, drag off it, release on nothing.
        press(50.0);
        release(150.0);
        assert_eq!(
            fired.get(),
            0,
            "a press dragged OFF its target must not activate"
        );

        // The complete click.
        press(50.0);
        release(60.0);
        assert_eq!(
            fired.get(),
            1,
            "press and release on the same target activates"
        );

        // Travel is unbounded unless asked for, so a swipe ACROSS one target still
        // activates on the default settings — the bound is opt-in, and the next test
        // proves it bites when taken.
        press(10.0);
        release(90.0);
        assert_eq!(
            fired.get(),
            2,
            "without max_travel a swipe within one target still completes"
        );

        // A cancelled sequence drops its press, so the next stray release finds none.
        press(50.0);
        gesture.emit_by_name::<()>("cancel", &[&None::<gtk::gdk::EventSequence>]);
        release(60.0);
        assert_eq!(
            fired.get(),
            2,
            "a cancelled sequence must not still complete on release"
        );
    }

    /// The link pane's configuration: with a travel bound, a swipe that begins and ends
    /// inside ONE target is a selection, not a click. Wired the same way the preview's
    /// link gesture is, so what is asserted here is what ships there.
    #[gtktest::test]
    fn a_bounded_activation_rejects_a_swipe_across_one_target() {
        let fired: Rc<Cell<u32>> = Rc::new(Cell::new(0));
        let gesture = gtk::GestureClick::new();
        let seen = Rc::clone(&fired);
        ClickActivation::new().max_travel(drag_threshold()).wire(
            &gesture,
            |_, _| Some(0u8),
            move |_, _target: u8, _, _| seen.set(seen.get() + 1),
        );
        let press = |x: f64| gesture.emit_by_name::<()>("pressed", &[&1i32, &x, &0.0f64]);
        let release = |x: f64| gesture.emit_by_name::<()>("released", &[&1i32, &x, &0.0f64]);

        press(100.0);
        release(400.0);
        assert_eq!(
            fired.get(),
            0,
            "a swipe-selection within one link must not activate it"
        );

        press(100.0);
        release(101.0);
        assert_eq!(fired.get(), 1, "an ordinary click still activates");

        assert!(
            drag_threshold() > 0.0,
            "the desktop's drag threshold must be a usable bound — a zero would make \
             every click a drag"
        );
    }
}
