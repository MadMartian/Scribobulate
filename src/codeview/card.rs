//! [`AnnotationCard`] — the annotation comment card, as a `GtkPopover` subclass that
//! owns its own anchoring, scroll-tracking and dismissal.
//!
//! # Why a subclass, and why still a popover
//!
//! The card used to be a bare `gtk::Popover` driven entirely from the outside: a caller
//! computed a rectangle, handed it to `set_pointing_to`, and called `popup()`. Three
//! defects came out of that one arrangement, and they are the same defect seen from three
//! angles — **the card was positioned in a coordinate space that expires.**
//! A widget rectangle is only meaningful at the scroll offset it was read at, and every
//! caller carried one across a scroll:
//!
//!  * Nothing re-pointed the card when the document scrolled, so it stranded over
//!    unrelated text. (This was NOT a behaviour the autohide seat grab used to supply and
//!    that GTK4Rs/AP-83 took away: 4.6.9's autohide check handles button and touch input only,
//!    never scroll, and GTK implements no anchor-tracking anywhere — ScrAP-188.)
//!  * Nothing dismissed it on an outside click, autohide having been switched off.
//!  * Activating a different annotation re-pointed the card with a rect captured *before*
//!    the navigation's converge-scroll, so it landed away from its chip (ScrAP-187's
//!    family: state captured at a time that expires, here in screen rather than source
//!    coordinates).
//!
//! The fix is not three patches; it is to stop storing rectangles. This widget anchors to
//! the annotation's **identity** (its `src_span.start`, the same stable key the annotations
//! viewer navigates by — never a `Vec` index, GTK4Rs/AP-74) and re-derives the chip rectangle
//! from the live document *every time it is about to be shown or the view scrolls*. A
//! stale rect is then not "a bug we fixed", it is a value that no longer exists.
//!
//! **It stays a `GtkPopover`, deliberately.** The alternatives were researched against the
//! GTK 4.6.9 C source and measured:
//!
//!  * `gtk_text_view_add_overlay` (the card as a child of the document, in *buffer*
//!    coordinates) is **disqualified**. Its doc comment promises "@child will scroll with
//!    the text view" and the implementation does not do it before GTK 4.19.1 — two
//!    independent short-circuits (`gtk_text_view_value_changed` only re-allocates when
//!    `anchored_children` is non-empty; `gtk_text_view_child_set_offset` ends in
//!    `queue_draw`, not `queue_allocate`) leave the child pinned. Worse, and fatally for
//!    us: `gtk_text_view_measure` measures `center_child`, and `gtk_text_view_child_measure`
//!    takes the **max over every overlay** — so a 280 px card raises the *view's minimum
//!    width*, which re-arms the marginal `Automatic` h-scrollbar and with it the whole
//!    GTK4Rs/AP-124/GTK4Rs/AP-23a → GTK4Rs/AP-22 churn-blank chain. There is no opt-out. And it would behave
//!    *differently* on our macOS build (4.22.4, where the scroll bug is fixed) than on our
//!    4.6.9 floor — the exact cross-platform divergence this rebuild exists to end.
//!    Full write-up: ScrAP-189.
//!  * A **popover parented with `set_parent` is in neither list** (`center_child` nor
//!    `anchored_children`), so it contributes nothing to the view's size request. The route
//!    we are on is structurally immune to the constraint that disqualified the other one.
//!
//! And it is what upstream does for this exact shape of affordance: GtkSourceView's
//! completion popup, hover and snippet tooltips are all `GtkSourceAssistant`, a
//! `GtkPopover` subclass parented to the view with `gtk_widget_set_parent`, with
//! `gtk_popover_set_autohide(…, FALSE)` — the same choice GTK4Rs/AP-83 forced on us — that
//! recomputes its position from a text position on every adjustment `value-changed` and
//! **hides when its anchor leaves the viewport**. This module is that pattern, with the
//! chip (not a bare text iter) as the anchor.
//!
//! # The structural guarantee, and the hole that is easy to leave in it
//!
//! The invariant is: **no route to being on screen can skip the recompute.** It takes two
//! enforcement points, not one, and the reason is worth stating because the obvious
//! single-point version looks complete and is not.
//!
//!  1. [`WidgetImpl::show`] recomputes before chaining up, copying
//!     `_gtk_source_assistant_show`. This covers every *transition* into visibility.
//!  2. [`present`](AnnotationCard::present) recomputes unconditionally, via
//!     [`reposition`](AnnotationCard::reposition).
//!
//! Point 2 is not redundant. `set_visible(true)` on an **already-visible** widget is a
//! no-op — it does not run the `show` vfunc — and "already visible" is precisely the state
//! ScrAP-190 describes: activating a different annotation while the card is up re-points the
//! existing card rather than building a new one (which is correct, and is why the popover
//! is reused). So a version that relies on the vfunc alone passes every open-from-closed
//! test and silently fails the one case the issue is about. That is not hypothetical; it is
//! what the first implementation here did, and what
//! `presenting_recomputes_the_anchor_rather_than_reusing_a_stale_rect` was written to catch
//! after it did. Mutation-test both points if you touch either (GTK4Rs/AP-78).
//!
//! With both in place a future caller cannot forget, because no code path asks it to
//! remember: there is no rectangle parameter anywhere in this widget's API to get wrong
//! (the GTK4Rs/AP-108/GTK4Rs/AP-130 enforcement reflex — the mitigation sits on the paths every
//! presentation must traverse, rather than being a rule each call site has to re-apply).

use super::CodePreviewView;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib::Propagation};
use std::cell::{Cell, RefCell};

mod imp {
    use super::*;

    #[derive(Default)]
    pub(crate) struct AnnotationCard {
        /// **Identity, not geometry and not an index.** The `src_span.start` of the first
        /// annotation this card is showing — the same stable key `marker_index_for_src`
        /// resolves for the annotations viewer. An index would go stale the moment the
        /// document re-renders and `markers` is rebuilt (GTK4Rs/AP-74); a rectangle would go
        /// stale the moment the view scrolls. An identity goes stale only when
        /// the annotation itself is gone, which is exactly when this card should stop
        /// pointing at anything.
        pub(super) anchor_src: Cell<Option<usize>>,

        /// Which marker indices are LISTED in the current content, for the
        /// redundant-re-open guard in `open_marker_popover` (a stray re-dispatch of the
        /// same marker must not rebuild the child over an in-progress Edit entry).
        pub(super) idxs: RefCell<Vec<usize>>,

        /// **Logically showing**, which is not the same as `is_visible()`: the card stays
        /// "open" while temporarily hidden because its chip scrolled off the viewport, so
        /// it can re-show itself when the chip comes back. It is also the lag-free answer
        /// for `has_open_marker_popover` (GTK4Rs/AP-117).
        pub(super) open: Cell<bool>,

        /// Set while the card is hidden because its chip is off the viewport, as opposed
        /// to dismissed. `GtkPopover` emits `closed` from its `hide` vfunc, and the
        /// `closed` backstop clears [`open`](Self::open) — so without this a scroll-off
        /// would end the session and the card could never come back when its chip
        /// scrolled into view again.
        ///
        /// **Sticky**: set when hiding, cleared when the card is shown again or genuinely
        /// dismissed — deliberately NOT scoped tightly around the `set_visible(false)`
        /// call. Scoping it there assumes `closed` is emitted synchronously from inside
        /// that call, which is an assumption about GTK's emission timing we do not need to
        /// make.
        pub(super) transient_hide: Cell<bool>,

        /// The vadjustment we track for scroll re-anchoring, and our handler on it. Re-resolved at each
        /// presentation rather than wired once, so a replaced adjustment cannot leave us
        /// "connected" to an orphan (ScrAP-52); disconnected at teardown so a per-render
        /// card never accumulates handlers on a longer-lived adjustment.
        pub(super) vadj: RefCell<Option<(gtk::Adjustment, glib::SignalHandlerId)>>,

        /// The toplevel we watch for outside clicks, and the gesture we put there.
        /// Held so teardown can remove it: the toplevel outlives this card, so leaving the
        /// controller behind would accumulate one per render.
        pub(super) outside: RefCell<Option<(gtk::Widget, gtk::GestureClick)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AnnotationCard {
        const NAME: &'static str = "ScribAnnotationCard";
        type Type = super::AnnotationCard;
        type ParentType = gtk::Popover;
    }

    impl ObjectImpl for AnnotationCard {}

    impl WidgetImpl for AnnotationCard {
        /// **Recompute the anchor, then present** — never the other way round.
        ///
        /// This is `_gtk_source_assistant_show`'s shape, and it is the FIRST of the two
        /// enforcement points the recompute invariant needs (see the module docs): it
        /// covers every *transition* into visibility — a first click on a chip, a
        /// programmatic navigation, a re-show after the chip scrolled back into view — so
        /// none of those can present a rectangle read at an earlier scroll offset. It does
        /// NOT cover a present on an already-visible card, because GTK skips this vfunc
        /// entirely in that case; [`present`](super::AnnotationCard::present) is the
        /// second point, and it is the one that covers the re-point path. There is no path
        /// that can present a stale rectangle, because no path carries
        /// one.
        ///
        /// Deliberately does NOT touch visibility (it only sets `pointing_to`): deciding
        /// to *hide* belongs to `update_position`, and calling `set_visible(false)` from
        /// inside the `show` vfunc would re-enter visibility handling mid-show.
        fn show(&self) {
            if let Some(rect) = self.obj().chip_rect_now() {
                self.obj().point_at(&rect);
            }
            self.parent_show();
        }
    }

    impl PopoverImpl for AnnotationCard {}
}

glib::wrapper! {
    pub(crate) struct AnnotationCard(ObjectSubclass<imp::AnnotationCard>)
        @extends gtk::Popover, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Native, gtk::ShortcutManager;
}

impl AnnotationCard {
    /// Build the card and parent it to `view`.
    ///
    /// `set_parent` (not a container `append`): a popover is a native child, which also
    /// means GTK will NOT unparent it for us — [`teardown`](Self::teardown) must, from the
    /// view's `dispose` (GTK4Rs/AP-80/GTK4Rs/AP-80), popping it down first (ScrAP-144).
    pub(crate) fn new(view: &CodePreviewView) -> Self {
        let card: Self = glib::Object::builder().build();
        // Non-autohide, like every other popover in this app and like BOTH of
        // GtkSourceView's non-autohide assistants. An autohide popover holds an X11 seat
        // grab that (GTK 4.6, researcher-confirmed) drops the popover→window coordinate
        // offset, so a mouse click on this card's OWN Edit/Remove buttons resolves as
        // OUTSIDE it and dismisses the card instead of activating the button — keyboard
        // worked only because it skips pointer routing (GTK4Rs/AP-83). Autohide also supplied
        // focus-on-open, Escape-dismiss and outside-click dismiss for free; the first two
        // are re-wired below and in `open_marker_popover`, and outside-click dismissal is
        // this widget's `arm_outside_dismiss` — which, unlike autohide, cannot misroute a
        // press onto our own buttons, because those presses are delivered inside the
        // card's own surface and never reach the toplevel gesture at all.
        card.set_autohide(false);
        card.set_position(gtk::PositionType::Left);
        card.set_parent(view);

        // Escape-dismiss, CAPTURE phase deliberately: a bubble-phase controller did NOT
        // dismiss once focus was in the edit entry (mac/Windows real-display testing) —
        // the focused child (a `GtkText`, or a button) consumes Escape before it bubbles
        // up. Catching it top-down, before any child sees it, dismisses from both the read
        // buttons and the edit entry.
        let esc = gtk::EventControllerKey::new();
        esc.set_propagation_phase(gtk::PropagationPhase::Capture);
        esc.connect_key_pressed(glib::clone!(
            #[weak]
            card,
            #[upgrade_or]
            Propagation::Proceed,
            move |_, keyval, _, _| {
                if keyval == gdk::Key::Escape {
                    card.dismiss();
                    Propagation::Stop
                } else {
                    Propagation::Proceed
                }
            }
        ));
        card.add_controller(esc);
        card.add_controller(app_accelerator_controller());

        // Backstop only. Nothing in GTK closes a non-autohide popover on its own, and
        // every deliberate dismissal routes through `dismiss()` — but `closed` is also
        // emitted from the `hide` vfunc, so this catches any close we did not initiate.
        // It must NOT fire for our own scroll-off hide, hence `transient_hide`.
        card.connect_closed(|c| {
            if !c.imp().transient_hide.get() {
                c.imp().open.set(false);
            }
        });
        card
    }

    /// The view this card is parented to, or `None` once torn down.
    fn view(&self) -> Option<CodePreviewView> {
        self.parent()
            .and_then(|p| p.downcast::<CodePreviewView>().ok())
    }

    /// Whether the card is logically showing an annotation — lag-free, and true even while
    /// it is transiently hidden because its chip is scrolled off (GTK4Rs/AP-117: never
    /// `is_visible()`, and here not even "is it on screen").
    ///
    /// This is the **session** question: does the user have an annotation open? Use it for
    /// lifecycle decisions (teardown, redundant-re-open suppression). For "would something
    /// else collide with it on screen", use [`is_showing`](Self::is_showing) — the two
    /// diverged the moment the card gained an off-viewport hidden state, and conflating
    /// them suppresses affordances against a card nobody can see.
    pub(crate) fn is_open(&self) -> bool {
        self.imp().open.get()
    }

    /// Whether the card is **on screen right now** — an open session AND not hidden for a
    /// scrolled-off anchor.
    ///
    /// The question every "don't stack another surface over it" gate actually asks. Those
    /// gates exist to stop two popovers occupying the same place at the same time, which
    /// is a statement about pixels, not about sessions: a card whose chip has scrolled out
    /// of view occupies nothing and must not suppress anything.
    pub(crate) fn is_showing(&self) -> bool {
        self.is_open() && self.is_visible()
    }

    /// The marker indices currently listed, for the redundant-re-open guard.
    pub(crate) fn shows_exactly(&self, idxs: &[usize]) -> bool {
        self.imp().idxs.borrow().as_slice() == idxs
    }

    /// Adopt `idxs` as the content this card is showing, anchored to `anchor_src` — the
    /// `src_span.start` of the group's first annotation. Called by `open_marker_popover`
    /// after it has rebuilt the child; the actual presentation is a plain
    /// [`present`](Self::present), which recomputes geometry on the way through `show`.
    pub(crate) fn adopt(&self, idxs: &[usize], anchor_src: usize) {
        *self.imp().idxs.borrow_mut() = idxs.to_vec();
        self.imp().anchor_src.set(Some(anchor_src));
    }

    /// Present the card. There is deliberately no rectangle parameter — the anchor is
    /// derived, never supplied (see the module docs).
    ///
    /// Ends in [`reposition`](Self::reposition) rather than a bare `set_visible(true)`,
    /// and that is load-bearing, not tidiness. `set_visible(true)` on an ALREADY-VISIBLE
    /// popover is a no-op, so it does not run the `show` vfunc — which means the vfunc's
    /// recompute would silently not happen on the one path the stale-anchor defect is about:
    /// **switching to another annotation while the card is already up.** (The card is
    /// reused and re-pointed rather than rebuilt, precisely because `set_pointing_to`
    /// re-presents a visible popover.) Going through `reposition` covers both cases with
    /// one call: it recomputes unconditionally, points, and shows or hides according to
    /// where the chip actually is — including hiding when the annotation has gone away
    /// entirely. The `show` override remains as the structural backstop for every OTHER
    /// route to visibility.
    pub(crate) fn present(&self) {
        self.imp().open.set(true);
        self.arm_scroll_tracking();
        self.arm_outside_dismiss();
        self.reposition();
    }

    /// Dismiss the card for good (Escape, an outside click, a committed Edit/Remove, or a
    /// caller that knows the view is going away). Clears the anchor state so a later
    /// presentation cannot flash at the rectangle this one ended on — the hygiene
    /// `gtk_source_hover_assistant_hide` performs for the same reason.
    pub(crate) fn dismiss(&self) {
        self.imp().open.set(false);
        self.imp().transient_hide.set(false);
        self.imp().anchor_src.set(None);
        self.set_visible(false);
        self.set_pointing_to(None);
        self.set_offset(0, 0);
    }

    /// Point the card at `rect`, skipping the call when it is already pointing there.
    ///
    /// The skip is not micro-optimisation: `set_pointing_to` on a *visible* popover
    /// re-presents it immediately (which is why re-pointing works at all), so an
    /// unguarded call on every `value-changed` would re-run the popup layout for scroll
    /// emissions that did not move the chip. Upstream guards the same call the same way.
    ///
    /// **Compares against the popover's ACTUAL anchor, not a remembered one.** Upstream's
    /// version — and this widget's first version — caches the last rectangle it wrote and
    /// compares against that. A write-cache is not a reading of the object: anything that
    /// sets `pointing_to` from outside desynchronises it, and the skip then suppresses
    /// exactly the recompute a presentation must perform. Reading the live value cannot
    /// desynchronise, costs a getter, and deletes a piece of state whose only job was to
    /// be wrong occasionally. `saferizer::pointing_to` is the sanctioned reader — the raw
    /// binding's `(bool, Rectangle)` hands back an undefined rectangle for an unset anchor,
    /// which is precisely the case this comparison meets on the very first present.
    fn point_at(&self, rect: &gdk::Rectangle) {
        let same = crate::saferizer::pointing_to(self).is_some_and(|cur| {
            (cur.x(), cur.y(), cur.width(), cur.height())
                == (rect.x(), rect.y(), rect.width(), rect.height())
        });
        if same {
            return;
        }
        self.set_pointing_to(Some(rect));
    }

    /// The chip's rectangle **right now**, in the view's widget coordinates, re-derived
    /// from the anchored annotation's identity. `None` when there is no anchor, no view,
    /// or the annotation is no longer in this render's markers (it was removed, or the
    /// document re-rendered without it) — the caller then hides rather than pointing at a
    /// position that no longer means anything.
    fn chip_rect_now(&self) -> Option<gdk::Rectangle> {
        let src = self.imp().anchor_src.get()?;
        let view = self.view()?;
        let index = view.marker_index_for_src(src)?;
        view.marker_chip_rect(index)
    }

    /// Re-anchor to the chip after the document has scrolled, hiding while the
    /// chip is off the viewport and re-showing when it returns.
    ///
    /// Safe to read geometry from here, and this is the ONE hook where that is true
    /// without a paint round-trip. `GtkTextView` connects its own handler to the
    /// vadjustment in `gtk_text_view_set_vadjustment` — i.e. before we can — and
    /// `gtk_text_view_value_changed` calls `gtk_text_view_validate_onscreen` synchronously
    /// and cancels the pending first-validate idle. GLib runs same-stage handlers in
    /// connection order, so by the time this runs the onscreen region is already validated
    /// for the new offset and the reposition lands in the same frame as the scroll.
    ///
    /// That guarantee is an *ordering* property of this specific signal, not a property of
    /// the geometry APIs, which are emphatically not self-validating: `iter_location`'s
    /// `ensure_layout` only ensures a layout *exists*, and `buffer_to_window_coords` is a
    /// bare `y -= yoffset` with no flush (GTK4Rs/AP-142/GTK4Rs/AP-142). Repositioning from a tick
    /// callback instead would sample at the UPDATE phase, before `size_allocate` runs
    /// `flush_first_validate` at LAYOUT — which is why the programmatic-navigation path
    /// uses `connect_after_paint` and not this hook.
    fn reposition(&self) {
        if !self.is_open() {
            return;
        }
        // `on_viewport` returns the anchor CLAMPED into the viewport, and taking the
        // clamped value is the point: an earlier version here asked a bare
        // "is it visible?" predicate and then pointed at the UNCLAMPED rectangle, which
        // passes the guard and still hands GTK a negative y whenever the chip straddles
        // the top edge — the GTK4Rs/AP-26 assertion the guard exists to prevent, arrived at
        // through the guard. The seam has no shape that lets you do that.
        let on_screen = self
            .view()
            .and_then(|v| Some((crate::saferizer::Viewport::of(&v), self.chip_rect_now()?)))
            .and_then(|(vp, r)| crate::saferizer::on_viewport(vp, &r))
            .inspect(|r| self.point_at(r));
        match on_screen {
            Some(_) => {
                self.imp().transient_hide.set(false);
                if !self.is_visible() {
                    self.set_visible(true);
                }
            }
            None => self.hide_transiently(),
        }
    }

    /// Hide without ending the card's session — it is still "open", it just has nothing on
    /// screen to point at. See [`imp::AnnotationCard::transient_hide`].
    ///
    /// The flag is set and left set — cleared when the card is shown again or genuinely
    /// dismissed — rather than being wrapped tightly around the `set_visible` call. Scoping
    /// it to the call assumes `closed` is emitted synchronously from inside it, which is an
    /// assumption about GTK's emission timing that we do not need to make and that a
    /// measured run does not support: the tightly-scoped version let the `closed` backstop
    /// clear `open`, which ended the session and made the card unable to come back when its
    /// chip scrolled into view again.
    fn hide_transiently(&self) {
        if !self.is_visible() {
            return;
        }
        self.imp().transient_hide.set(true);
        self.set_visible(false);
    }

    /// Track the view's vadjustment, re-resolving it each time rather than wiring once: a
    /// replaced adjustment would otherwise leave us "connected" to an orphan while
    /// appearing wired (ScrAP-52).
    fn arm_scroll_tracking(&self) {
        let Some(view) = self.view() else { return };
        let Some(vadj) = view.vadjustment() else {
            return;
        };
        if let Some((have, _)) = self.imp().vadj.borrow().as_ref() {
            if have == &vadj {
                return;
            }
        }
        self.disarm_scroll_tracking();
        // WEAK: the adjustment outlives the card, and a strong capture here would make
        // the card uncollectable for as long as the scroller lives (ScrAP-60/ScrAP-155).
        let id = vadj.connect_value_changed(glib::clone!(
            #[weak(rename_to = card)]
            self,
            move |_| card.reposition()
        ));
        self.imp().vadj.replace(Some((vadj, id)));
    }

    fn disarm_scroll_tracking(&self) {
        if let Some((adj, id)) = self.imp().vadj.borrow_mut().take() {
            adj.disconnect(id);
        }
    }

    /// Dismiss on a click anywhere outside the card.
    ///
    /// A CAPTURE-phase gesture on the TOPLEVEL — not on the text view, because a click on
    /// the toolbar, the editor pane or the sidebar must dismiss the card too and a
    /// view-level gesture would only cover the document. It never claims the gesture:
    /// dismissing is a side effect, and the click must still reach whatever it was aimed
    /// at.
    ///
    /// # Two exclusions, and why the first one is kept even though it "can't" be needed
    ///
    /// **1. A press inside the card's own surface.** On X11/4.6.9 this exclusion is, as far
    /// as anyone can make it fire, dead code — and it stays in anyway. Measured with real
    /// pointer input: a press inside a non-autohide popover reaches a capture controller on
    /// the popover itself and reaches *nothing* above it — not the popover's widget-parent,
    /// not the toplevel — while a control press on the window does reach the toplevel, so
    /// the gesture is wired correctly and delivery genuinely stops at the `GtkNative`
    /// boundary. That was checked with `GtkEventControllerLegacy` (no gesture recognition,
    /// no coordinate filtering), so it is delivery stopping, not a gesture declining.
    ///
    /// But **the behaviour is verified and the mechanism is not.** `gtk_propagate_event`
    /// walks widget parents up to the current grab, a non-autohide popover installs no grab
    /// (`gtk_grab_add` sits inside the `autohide` branch), and every ancestor here is
    /// realized — so the source reads as though the press *should* reach the window, and
    /// measurably it does not. Nobody has located the line that stops it. Nor has any of
    /// this been measured on Quartz or Win32, which is exactly where this widget's history
    /// of pain comes from.
    ///
    /// A behaviour you can demonstrate but cannot explain is not a contract to build on,
    /// and the failure mode if it does not hold on some backend is precisely the bug this
    /// app already shipped once: a press on the card's own button resolving as "outside"
    /// and dismissing the card instead of activating it — which is what forced `autohide`
    /// off (GTK4Rs/AP-83). Three lines is a cheap price for not re-litigating that on a
    /// platform we cannot test from here. The discriminator is surface identity, which
    /// answers the question directly and is correct under every reading.
    ///
    /// **2. A press on a marker chip.** Without it, clicking a chip would dismiss the open
    /// card here (capture runs first) and the view's own handler would then rebuild and
    /// re-open it — wiping an in-progress Edit entry, because the redundant-re-open guard
    /// keys on the card still being open.
    fn arm_outside_dismiss(&self) {
        let Some(root) = self.root().map(|r| r.upcast::<gtk::Widget>()) else {
            return;
        };
        if let Some((have, _)) = self.imp().outside.borrow().as_ref() {
            if have == &root {
                return;
            }
        }
        self.disarm_outside_dismiss();
        let click = gtk::GestureClick::new();
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        click.set_button(0); // any button
        click.connect_pressed(glib::clone!(
            #[weak(rename_to = card)]
            self,
            move |gesture, _, x, y| {
                if !card.is_open() || card.press_is_ours(gesture, x, y) {
                    return;
                }
                card.dismiss();
            }
        ));
        root.add_controller(click.clone());
        self.imp().outside.replace(Some((root, click)));
    }

    fn disarm_outside_dismiss(&self) {
        if let Some((root, click)) = self.imp().outside.borrow_mut().take() {
            root.remove_controller(&click);
        }
    }

    /// Whether a press seen by the toplevel outside-dismiss gesture belongs to the card's
    /// own interaction and must therefore NOT dismiss it — the single predicate behind
    /// both exclusions documented on [`arm_outside_dismiss`](Self::arm_outside_dismiss).
    ///
    /// Kept as one named function rather than inlined into the closure so the rule has a
    /// place to be stated, and so a future third exclusion has an obvious home instead of
    /// growing another `if` in the handler (GTK4Rs/AP-108: the mitigation lives at one choke
    /// point, not scattered across call sites).
    fn press_is_ours(&self, gesture: &gtk::GestureClick, x: f64, y: f64) -> bool {
        // (1) Delivered into the card's OWN popup surface. Surface identity answers this
        // directly, whatever GTK's propagation does with the widget hierarchy.
        let ours = self.surface();
        let from = gesture.current_event().and_then(|e| e.surface());
        if let (Some(ours), Some(from)) = (ours, from) {
            if ours == from {
                return true;
            }
        }
        // (2) On a marker chip — the view's own click handler owns that press, and letting
        // it dismiss here would tear down a card the view is about to re-open.
        let Some(view) = self.view() else {
            return false;
        };
        let Some(widget) = gesture.widget() else {
            return false;
        };
        widget
            .translate_coordinates(&view, x, y)
            .is_some_and(|(vx, vy)| view.is_over_marker(vx as f32, vy as f32))
    }

    /// Tear the card down from the view's `dispose`.
    ///
    /// Two halves, and only one of them is this widget's own business:
    ///
    ///  * **Ours** — drop the handlers we planted on objects that OUTLIVE us: the
    ///    scroller's adjustment and the toplevel. Nothing else will; a card per render
    ///    that left them behind would accumulate one of each on a longer-lived object.
    ///  * **Not ours** — the parenting teardown, which must be **popdown before unparent**
    ///    (ScrAP-144), with the unparent mandatory because GTK does not unparent a
    ///    `set_parent`-attached popover for us (GTK4Rs/AP-80/GTK4Rs/AP-80). That order already has a
    ///    seal — [`PersistentPopover`](crate::saferizer::PersistentPopover) exists so it
    ///    cannot be written wrongly — so it is **delegated**, not re-implemented here. Two
    ///    hand-written copies of one invariant is how the invariant drifts, and the copy
    ///    that drifts is always the one that isn't the seal.
    ///
    /// The handle is built transiently rather than stored in the card's own state: holding
    /// a `PersistentPopover` over `self` would be a strong self-reference, and the card
    /// would never reach finalize.
    ///
    /// Not `Drop`: that fires at GObject *finalize*, after `dispose`, when the parent may
    /// already be gone — too late to unparent safely.
    pub(crate) fn teardown(&self) {
        self.disarm_scroll_tracking();
        self.disarm_outside_dismiss();
        self.imp().open.set(false);
        crate::saferizer::PersistentPopover::adopt(self.clone().upcast()).teardown();
    }
}

/// A `GtkShortcutController` re-offering every application accelerator locally, so the
/// app's keyboard stays alive while this card holds the focus.
///
/// **Why a focused popover needs its own copy at all** (ScrAP-266). This card is `set_parent`ed and
/// therefore its own `GtkNative` — a widget-tree descendant of the view, but a separate
/// GDK surface. A key pressed while it has focus is delivered to *that* surface and never
/// reaches the window surface's handling, where `gtk_application_set_accels_for_action`'s
/// bindings live. MEASURED (4.6.9/X11): with the card focused, **no** application
/// accelerator fires — not the annotation walk, not even an unrelated `F8` pane toggle —
/// and the identical keystroke works the instant Escape returns focus to the view. So the
/// card was a keyboard dead zone for as long as it was open, which is precisely how long
/// a reader walking annotations keeps it open.
///
/// **BUBBLE phase, deliberately.** The application's own accelerators are effectively
/// global-scope and beat a focused `GtkText`'s bindings (ScrAP-137) — which is a hazard
/// this controller must not re-import from the other side, because the card hosts a real
/// text entry while its comment is being edited. A bubble-phase controller is offered the
/// key only after the focused widget has declined it, so `Ctrl+A` in the comment entry
/// stays the entry's and never reaches here.
///
/// The bindings come from [`crate::app::accelerator_bindings`] — the same table
/// `register_accelerators` binds from — so this cannot advertise a key the app does not
/// bind, or miss one it does. A `GtkNamedAction` resolves through the widget's action
/// muxer, which walks the widget-tree parents to the window, so the `win.` actions are
/// found even though the surface is not the window's.
fn app_accelerator_controller() -> gtk::ShortcutController {
    let controller = gtk::ShortcutController::new();
    controller.set_propagation_phase(gtk::PropagationPhase::Bubble);
    for (action, accel) in crate::app::accelerator_bindings() {
        let Some(trigger) = gtk::ShortcutTrigger::parse_string(&accel) else {
            continue; // an unparseable accel is already inert app-wide; nothing to add
        };
        controller.add_shortcut(gtk::Shortcut::new(
            Some(trigger),
            Some(gtk::NamedAction::new(&action)),
        ));
    }
    controller
}

/// Live-display tests for the annotation card's anchoring contract.
/// Need a real display and a real paint; run with `cargo test --features
/// gtk-integration-tests`, under Xvfb if headless.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;
    use crate::annotations::Direction;
    use crate::codeview::CodePreviewView;

    /// One annotation near the top and a long tail of filler, so the chip can be scrolled
    /// off the viewport and back within one window.
    const MD: &str = "Intro paragraph here.\n\n\
        The {==first claim==}{>>first note<<} sits near the top.\n\n\
        filler\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\n\
        filler\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\n\
        filler\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\n\
        filler\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\n";

    /// The card's current anchor, or `None` when it has none — via the seam, because the
    /// raw binding's `(bool, Rectangle)` tuple carries an undefined rectangle when the
    /// bool is false.
    fn anchor(card: &AnnotationCard) -> Option<gdk::Rectangle> {
        crate::saferizer::pointing_to(card)
    }

    /// The first descendant `GtkButton` whose label is `label`.
    fn find_button(root: &impl IsA<gtk::Widget>, label: &str) -> Option<gtk::Button> {
        let mut child = root.as_ref().first_child();
        while let Some(c) = child {
            if let Ok(b) = c.clone().downcast::<gtk::Button>() {
                if b.label().as_deref() == Some(label) {
                    return Some(b);
                }
            }
            if let Some(found) = find_button(&c, label) {
                return Some(found);
            }
            child = c.next_sibling();
        }
        None
    }

    /// Depth-first search for the first descendant of type `T`.
    fn find_descendant<T: IsA<gtk::Widget>>(root: &impl IsA<gtk::Widget>) -> Option<T> {
        let mut child = root.as_ref().first_child();
        while let Some(c) = child {
            if let Ok(t) = c.clone().downcast::<T>() {
                return Some(t);
            }
            if let Some(found) = find_descendant::<T>(&c) {
                return Some(found);
            }
            child = c.next_sibling();
        }
        None
    }

    /// The card's corner close button, found by the icon name it is built with. Walking
    /// the tree is right in a test — production must never re-discover built objects
    /// (ScrAP-10), but a test driving the real UI has no forwarded handle by design.
    fn find_close_button(root: &impl IsA<gtk::Widget>) -> Option<gtk::Button> {
        let mut child = root.as_ref().first_child();
        while let Some(c) = child {
            if let Ok(b) = c.clone().downcast::<gtk::Button>() {
                if b.icon_name().as_deref() == Some(crate::icons::Icon::WindowClose.name()) {
                    return Some(b);
                }
            }
            if let Some(found) = find_close_button(&c) {
                return Some(found);
            }
            child = c.next_sibling();
        }
        None
    }

    fn view_of(pane: gtk::Widget) -> CodePreviewView {
        pane.downcast::<gtk::Overlay>()
            .expect("preview pane is a GtkOverlay")
            .child()
            .and_then(|c| c.downcast::<gtk::ScrolledWindow>().ok())
            .expect("overlay wraps the scroller")
            .child()
            .and_then(|c| c.downcast::<CodePreviewView>().ok())
            .expect("scroller holds the CodePreviewView")
    }

    /// Pump the main loop until `done` reports true, or panic naming `what`.
    /// `crate::testpump::until` under `Clock::Frame` — everything these tests wait on is
    /// popover open/close, a re-anchor, or a repaint, all of which advance on the GTK
    /// frame clock and so need real elapsed time rather than a tight spin (see that enum
    /// variant's doc).
    ///
    /// **There is deliberately no turn budget.** The parameter this replaces was a turn
    /// count multiplied by 4 ms to "keep the same worst-case ceiling", which is exactly
    /// the substitution GTK4Rs/AP-261 rejects: turns are not time. The macOS seat
    /// MEASURED the spread on the same 200 `iteration(false)` turns — 74 ms with one live
    /// toplevel, 610 ms with three more alive — so the per-turn cost moves ~8x with
    /// machine load and no constant converts one into the other. A budget picked on one
    /// machine is therefore either a flake on a busier one or a needlessly slow suite on
    /// an idle one, and neither reading is visible at the call site.
    ///
    /// Failing to converge is a test bug at every call site below, so `until`'s generous
    /// per-clock failure bound (GTK4Rs/AP-122 — a bound, never the completion signal) is
    /// the right shape: `done()` observing real state is always what ends the wait, and a
    /// timeout panics naming what it was waiting FOR instead of letting the next
    /// assertion fail for the wrong reason.
    fn settle(what: &str, done: impl FnMut() -> bool) {
        crate::testpump::until(crate::testpump::Clock::Frame, what, done);
    }

    /// How long to watch for a wrong-show that would arrive DEFERRED.
    ///
    /// A negative claim's bound is the whole of its strength: too short and the test
    /// manufactures its own pass. Measured against the same frame-clock-plus-idle chain a
    /// wrong `reposition` would show the card through, with generous margin — never a turn
    /// count reinterpreted as milliseconds (GTK4Rs/AP-261).
    const DEFERRED_SHOW_WATCH: std::time::Duration = std::time::Duration::from_millis(500);

    /// How long a deliberate drain gives a queued redraw to reach the screen.
    ///
    /// A real duration measured against the mechanism it waits on — the GTK frame clock,
    /// which is what a `queue_draw` schedules against. A tick is one refresh period
    /// apart (~17 ms at the 60 Hz the clock targets when it has no presentation time to
    /// go on, as under Xvfb), and the paint that rewrites the marker hit-box map lands on
    /// one of them. Fifteen periods is generous for one view's redraw while still costing
    /// a quarter of a second. It is expressly NOT a turn count reinterpreted as
    /// milliseconds — see [`settle`] for why that conversion has no valid multiplier.
    const REDRAW_DRAIN: std::time::Duration = std::time::Duration::from_millis(250);

    /// Scroll the view the way a HAND does.
    ///
    /// Not a bare `vadj.set_value`: opening the card arms the GTK4Rs/AP-118 re-pin guard, which
    /// deliberately drags the scroll back for a window after the popup so a deferred
    /// validation scroll cannot move the document under the just-opened card. That guard
    /// stands down for a user scroll, and every real hand-scroll input source (wheel,
    /// scrollbar drag, PageUp/Down) says so by setting `user_scrolling` — so a test that
    /// sets the value without it is not simulating a user, it is simulating the very
    /// validation scroll the guard exists to defeat, and it measures the guard rather than
    /// the card.
    fn scroll_like_a_user(view: &CodePreviewView, to: f64) {
        use gtk::subclass::prelude::*;
        view.imp().user_scrolling.set(true);
        crate::saferizer::scrollpos::jump(
            &view.vadjustment().expect("the view is in a scroller"),
            to,
        );
    }

    /// A presented window showing the fixture, with the card open on its first annotation.
    /// Returns `(window, view, card)`; the window is returned so the caller keeps it alive.
    fn open_card() -> (gtk::Window, CodePreviewView, AnnotationCard) {
        use gtk::subclass::prelude::*;
        let pane = crate::preview::render(MD, None, 1.0, false);
        let view = view_of(pane.clone());
        let win = gtk::Window::new();
        win.set_default_size(700, 300);
        win.set_child(Some(&pane));
        win.present();
        // Precondition: the hit-box map is written BY the paint, so there is nothing to
        // open a card against until the view has actually painted once.
        settle("the marker chip to be painted", || {
            !view.imp().marker_hitboxes.borrow().is_empty()
        });
        assert!(
            view.open_stepped_marker_popover(0, Direction::Next),
            "precondition: the fixture has an annotation"
        );
        // Precondition: the card opened.
        settle("the marker card to open", || view.has_open_marker_popover());
        let card = view
            .imp()
            .marker_card
            .borrow()
            .clone()
            .expect("the card exists once opened");
        (win, view, card)
    }

    /// **The card follows the document.**
    ///
    /// Scrolling the view must move the card's anchor by the same amount, because the
    /// anchor is re-derived from the annotation rather than remembered from the open. This
    /// is the assertion the old code could not pass at all: `pointing_to` was written once
    /// and never touched again, so the card stayed put while the text slid under it.
    ///
    /// Asserted on `pointing_to` (the anchor the card is actually pinned to), not on a
    /// screenshot — the rect IS the mechanism, and the pixels are downstream of it.
    #[gtktest::test]
    fn the_card_re_anchors_when_the_document_scrolls() {
        let (win, view, card) = open_card();
        let before = anchor(&card).expect("an open card points somewhere");
        let vadj = view.vadjustment().expect("the view is in a scroller");

        // A modest scroll that keeps the chip on screen. Small enough that the chip stays
        // inside the viewport, so the card must RE-ANCHOR rather than take the hide branch.
        let delta = 20.0;
        scroll_like_a_user(&view, vadj.value() + delta);
        settle(
            "the card's anchor to move off where it was before the scroll",
            || anchor(&card).is_some_and(|r| r.y() != before.y()),
        );

        let after = anchor(&card).expect("still anchored after the scroll");
        assert_eq!(
            after.y(),
            before.y() - delta as i32,
            "the anchor must track the document: scrolling down by {delta} moves the chip \
             (and therefore the card's anchor) up by the same amount. Equal y here is the \
             regression — a rect written once at open and never re-derived."
        );
        assert!(card.is_visible(), "an on-screen chip keeps the card up");
        win.destroy();
    }

    /// **The card releases the document.**
    ///
    /// Scrolling the chip off the viewport hides the card (it has nothing to point at) but
    /// does NOT end its session, so scrolling back brings it straight back. Hiding rather
    /// than dismissing is upstream's behaviour for a view-anchored assistant, and it is
    /// what stops the card stranding over unrelated text.
    ///
    /// The re-show half is the one that discriminates a real fix from a lazy one: a
    /// "popdown on any scroll" patch passes the hide assertion and fails this.
    #[gtktest::test]
    fn the_card_hides_off_viewport_and_returns_with_its_chip() {
        let (win, view, card) = open_card();
        let vadj = view.vadjustment().expect("the view is in a scroller");
        let home = vadj.value();

        // Scroll to the end PROGRESSIVELY, re-aiming as the document grows. A single
        // `set_value(upper - page_size)` lands almost nowhere: line heights validate
        // lazily, so `upper` at this instant still describes a fraction of the document
        // and the value clamps to that fraction (GTK4Rs/AP-115). Re-aiming each frame is what
        // converges — and is what a hand-drag to the bottom does anyway.
        // Hiding is the behaviour under test: with its chip scrolled off the viewport the
        // card must hide rather than sit pinned over unrelated text, so never converging
        // here is the regression itself, not a slow machine.
        settle(
            "the card to hide once its chip is scrolled off the viewport",
            || {
                if card.is_visible() {
                    scroll_like_a_user(&view, vadj.upper() - vadj.page_size());
                }
                !card.is_visible()
            },
        );
        assert!(
            card.is_open(),
            "hiding for a scroll-off must NOT end the session — the card is still open, \
             it just has nothing on screen to point at. Losing this makes the return \
             below impossible."
        );
        assert!(
            !card.is_showing(),
            "…but it must NOT report itself as ON SCREEN while hidden. Every \
             'don't stack another surface over the card' gate asks this question, and \
             answering it with the session state suppresses the create-Annotate \
             affordance against a card the user cannot see — with nothing visible to \
             explain why. Two questions, two answers."
        );

        scroll_like_a_user(&view, home);
        // The re-show half: bringing the chip back into view must bring its card back —
        // this is the assertion a "popdown on any scroll" patch fails.
        settle("the card to come back with its chip", || card.is_visible());
        win.destroy();
    }

    /// **A presentation cannot use the rectangle it happens to be holding (ScrAP-190).**
    ///
    /// The defect was that a rect read from the painted hit-boxes was carried across a
    /// navigation's converge-scroll and then handed to `set_pointing_to`, so the card
    /// landed where the chip *had been*. The fix is structural: presenting re-derives the
    /// anchor from the annotation, so a rectangle that predates anything cannot survive to
    /// be presented.
    ///
    /// Proven by doing deliberately, and unmistakably, what the old code did by accident:
    /// poison `pointing_to` with a rect that is nowhere near the chip, then present, and
    /// assert the poison is gone AND that what replaced it is the chip's true position.
    ///
    /// The poison is injected rather than produced by a scroll on purpose. A scroll would
    /// make the test depend on how far the view actually scrolled, which on a freshly-built
    /// view is a function of lazy height validation (GTK4Rs/AP-115) rather than of what the fix
    /// does — a flaky measurement of the wrong thing. Injecting states the property
    /// directly: *whatever* the card is holding, presenting replaces it with the truth.
    ///
    /// The card stays VISIBLE throughout, also on purpose: that is the case ScrAP-190 is
    /// actually about (activating another annotation re-points a card that is already up),
    /// and it is the case a `set_visible(true)`-based fix silently misses, because showing
    /// an already-visible widget does not run the `show` vfunc at all. Mutation guards:
    /// revert `present` to `set_visible(true)` and this fails; drop the `chip_rect_now()`
    /// recompute and it fails.
    #[gtktest::test]
    fn presenting_recomputes_the_anchor_rather_than_reusing_a_stale_rect() {
        let (win, view, card) = open_card();
        let truth = view.marker_chip_rect(0).expect("the chip resolves");
        assert!(card.is_visible(), "precondition: the card is up");

        // A rectangle describing where the chip is NOT — the shape of every stale anchor
        // this design exists to make impossible.
        let poison = gdk::Rectangle::new(truth.x(), truth.y() + 137, truth.width(), truth.height());
        card.set_pointing_to(Some(&poison));
        assert_eq!(
            anchor(&card).map(|r| r.y()),
            Some(poison.y()),
            "precondition: the card really is holding the stale rect"
        );

        // Present, exactly as activating another annotation does while the card is up.
        card.present();
        settle("presenting to replace the poisoned anchor", || {
            anchor(&card).is_some_and(|r| r.y() != poison.y())
        });

        let shown = anchor(&card).expect("the card points somewhere");
        assert_ne!(
            shown.y(),
            poison.y(),
            "presenting must re-derive the anchor from the annotation, never present the \
             rectangle it happened to be holding"
        );
        assert_eq!(
            shown.y(),
            truth.y(),
            "and what it re-derived must be the chip's ACTUAL position — matching what the \
             painter would draw, since both go through `marker_row_y_h`/`chip_rect`"
        );
        win.destroy();
    }

    /// **Outside-click dismissal is armed, and is cleaned up.**
    ///
    /// A synthetic click cannot be delivered to a popover surface headlessly, so this
    /// asserts the STATE that makes the affordance exist — the capture-phase gesture is on
    /// the toplevel while the card is open, and is GONE after teardown — rather than the
    /// symptom. That is the durable half anyway: the gesture leaking onto a toplevel that
    /// outlives the view is a per-render accumulation no click test would ever catch, and
    /// the pointer behaviour itself needs the real-display pass (GTK4Rs/AP-56).
    #[gtktest::test]
    fn outside_dismiss_is_armed_on_the_toplevel_and_removed_at_teardown() {
        let (win, _view, card) = open_card();
        let root = card
            .root()
            .expect("the card is rooted")
            .upcast::<gtk::Widget>();

        let armed = root
            .observe_controllers()
            .into_iter()
            .flatten()
            .filter_map(|c| c.downcast::<gtk::GestureClick>().ok())
            .any(|g| g.propagation_phase() == gtk::PropagationPhase::Capture);
        assert!(
            armed,
            "an open card must arm a CAPTURE-phase click gesture on the TOPLEVEL — not on \
             the text view, or a click on the toolbar or the editor pane would not dismiss \
             it"
        );

        card.teardown();
        let still_there = root
            .observe_controllers()
            .into_iter()
            .flatten()
            .filter_map(|c| c.downcast::<gtk::GestureClick>().ok())
            .any(|g| g.propagation_phase() == gtk::PropagationPhase::Capture);
        assert!(
            !still_there,
            "teardown must take the gesture back off the toplevel: the window outlives the \
             view, so a card that leaves its controller behind accumulates one per render"
        );
        assert!(
            card.parent().is_none(),
            "teardown must also unparent the popover — GTK does not unparent a \
             `set_parent`-attached popover for us (GTK4Rs/AP-80/GTK4Rs/AP-80)"
        );
        win.destroy();
    }

    /// **Cancel aborts the edit; it does not close the card.**
    ///
    /// Three assertions, because "Cancel" has three plausible-but-wrong behaviours and only
    /// one of them is loud. It used to dismiss the whole card, which conflated "I don't
    /// want this edit" with "I'm done with this annotation" and answered both with the
    /// destructive one. So: the card must still be open, it must be back on the READ page
    /// (not merely open but stuck in edit), and the abandoned draft must not survive to
    /// greet the next Edit — otherwise Cancel is a postponement, not an abort.
    #[gtktest::test]
    fn cancel_returns_the_card_to_its_read_state_and_discards_the_draft() {
        let (win, view, card) = open_card();
        view.set_annotation_sink(std::rc::Rc::new(|_| {}));
        card.dismiss();
        assert!(
            view.open_stepped_marker_popover(0, Direction::Next),
            "reopen with the sink installed"
        );
        // Precondition: the card reopened, this time with Edit/Remove.
        settle("the card to reopen with Edit/Remove", || card.is_open());

        // The STACK's visible page is the discriminator throughout — never the presence of
        // a button or an entry. A GtkStack keeps every page parented whether shown or not,
        // so searching the card for "Edit" (or for the entry) succeeds on either page:
        // assertions and pump conditions written that way cannot fail and cannot wait.
        // (Both were, first time round; this test found them.)
        let stack = find_descendant::<gtk::Stack>(&card).expect("the row hosts a GtkStack");
        let entry = find_descendant::<gtk::Entry>(&card).expect("the edit page is prebuilt");
        let original = entry.text().to_string();

        find_button(&card, "Edit")
            .expect("the card offers Edit")
            .emit_clicked();
        // Precondition: Edit put the card on the edit page. The page swap is deferred to
        // an idle, so it is not observable on the turn the click is emitted.
        settle("Edit to put the card on the edit page", || {
            stack.visible_child_name().as_deref() == Some("edit")
        });
        entry.set_text("a draft the user thinks better of");

        find_button(&card, "Cancel")
            .expect("the edit page offers Cancel")
            .emit_clicked();
        settle("Cancel to return the card to the read page", || {
            stack.visible_child_name().as_deref() == Some("read")
        });

        assert!(
            card.is_open(),
            "Cancel must abort the EDIT, not the card — dismissing here costs the reader \
             their place over an edit they merely backed out of"
        );
        assert_eq!(
            stack.visible_child_name().as_deref(),
            Some("read"),
            "…and it must be back on the read page, not left open on the edit page it was \
             asked to leave"
        );
        assert_eq!(
            entry.text().as_str(),
            original,
            "the abandoned draft must not survive — a Cancel that keeps the text is a \
             postponement, and the next Edit would open on words the user discarded"
        );
        win.destroy();
    }

    /// The corner **×** dismisses the card — and does NOT take the focus that belongs to
    /// Edit.
    ///
    /// Two assertions because the button has two ways to be wrong, and the second is the
    /// silent one. Dismissal is the feature. The focus contract is the regression risk: the
    /// card focuses its first focusable descendant a tick after presenting so Tab/Space
    /// reach Edit/Remove, and the × is placed FIRST in the tree — so the moment it becomes
    /// focusable it captures that focus and the card opens one Space away from closing
    /// itself. Nothing about the visible result would reveal that; only this assertion
    /// would. Mutation guard: drop the `set_can_focus(false)` and the focus assertion
    /// fails while the dismissal one still passes.
    #[gtktest::test]
    fn the_corner_close_button_dismisses_without_stealing_focus() {
        let (win, view, card) = open_card();

        // Edit/Remove only exist once a mutation sink is installed — without one there is
        // nothing for the × to steal focus from, and the test would prove half its point.
        view.set_annotation_sink(std::rc::Rc::new(|_| {}));
        card.dismiss();
        assert!(
            view.open_stepped_marker_popover(0, Direction::Next),
            "reopen with the sink installed"
        );
        // Precondition: the card reopened.
        settle("the card to reopen", || card.is_open());

        let close = find_close_button(&card).expect("the card offers a corner close button");
        assert!(
            !close.can_focus(),
            "the close button must stay out of the tab ring: it is placed first in the \
             card, so a focusable one captures the deferred focus meant for Edit and the \
             card opens with 'close' selected. Escape is the keyboard route; this button \
             is the pointer's."
        );

        close.emit_clicked();
        // Dismissal is the feature: clicking the corner × must dismiss the card, like
        // Escape. Never converging here is that feature missing.
        settle("the corner × to dismiss the card", || !card.is_open());
        win.destroy();
    }

    /// The card must stop pointing at an annotation that no longer exists, rather than
    /// pinning itself to whatever marker now happens to occupy that index.
    ///
    /// This is why the anchor is an IDENTITY (`src_span.start`) and not a `Vec` index: the
    /// markers list is rebuilt on every render, so an index silently retargets while an
    /// identity simply stops resolving (GTK4Rs/AP-74, the ScrAP-187 family).
    #[gtktest::test]
    fn an_anchor_whose_annotation_is_gone_resolves_to_nothing() {
        use gtk::subclass::prelude::*;
        let (win, view, card) = open_card();
        assert!(
            anchor(&card).is_some(),
            "precondition: the card is anchored"
        );

        // Drop every marker, as an in-place re-render of a document whose annotation was
        // removed would. The identity now resolves to nothing.
        view.imp().markers.borrow_mut().clear();
        view.queue_draw();
        // A deliberate drain, not a wait for anything observable: the marker list is
        // already empty synchronously, and what this gives time for is the queued redraw
        // that repaints the view (and rewrites the hit-box map) without those markers.
        // There is no predicate to converge on, so the bound IS the wait — see
        // [`REDRAW_DRAIN`] for what that span is measured against.
        crate::testpump::drain_for(crate::testpump::Clock::Frame, REDRAW_DRAIN);

        card.present();

        // With its annotation gone the card must have nothing to anchor to and stay down,
        // not pin itself to a stale or reused position. `present` ends in `reposition`,
        // which hides on an unresolvable anchor.
        //
        // **Watched for, not waited on — and the distinction is the whole guard.** This
        // was `settle(.., || !card.is_visible())`. That form catches a SYNCHRONOUS
        // wrong-show and nothing else: `present` ends in `reposition`, which hides on an
        // unresolvable anchor before returning, so on the passing path the predicate is
        // already true the first time the wait evaluates it (`testpump` checks `done()`
        // ahead of its first `iteration`) and the loop pumps ZERO turns. It therefore
        // spends no time at all in the window where a DEFERRED wrong-show would land —
        // and `reposition` genuinely does run again off the frame clock, which is where
        // such a show would come from.
        //
        // INFERRED from those two code paths, not measured: I did not manage to stage a
        // deferred wrong-show that reproduces it, so the escape is argued rather than
        // demonstrated. What IS measured is the corrective's strength — deleting the
        // production `hide_transiently()` below makes this assertion fire. The general
        // rule is worth more than this instance either way: a claim about a NON-event
        // cannot be spelled as a wait for the good state, because such a wait is
        // satisfied instantly and proves only that the bad state is not ALREADY true. It
        // has to spend real time looking for the bad one.
        let showed = crate::testpump::until_or_for(
            crate::testpump::Clock::Frame,
            DEFERRED_SHOW_WATCH,
            || card.is_visible(),
        );
        assert!(
            !showed,
            "a card whose annotation is gone must stay down — it came up, so it anchored \
             to a stale or reused position instead of resolving to nothing"
        );
        win.destroy();
    }

    /// **The card must not be a keyboard dead zone.** Every application accelerator is
    /// re-offered locally, from the one table that binds them app-wide.
    ///
    /// A `set_parent`ed popover is its own `GtkNative`, and a key pressed while it holds
    /// focus never reaches the window's accelerator machinery — MEASURED on 4.6.9/X11: with
    /// the card focused, not even an unrelated `F8` pane toggle fired, and the same
    /// keystroke worked the instant Escape returned focus to the view. So the card owns a
    /// local copy or the app has no keyboard for as long as a comment is open.
    ///
    /// Asserted against `accelerator_bindings()` rather than a hand-listed set, because a
    /// hand-listed one is a second copy of the accel SSOT and would pass while the real
    /// answer drifted. The phase assertion is the other half: BUBBLE is what keeps a
    /// focused comment entry's own `Ctrl+A` its own (ScrAP-137 from the other side), and
    /// nothing about the visible result would reveal it had been changed.
    #[gtktest::test]
    fn the_card_re_offers_every_application_accelerator_locally() {
        let (win, _view, card) = open_card();

        // A `GtkPopover` carries shortcut controllers of GTK's own (measured: four on a
        // bare card), so ours is identified by what it OFFERS — the `win.` named actions —
        // not by being the only one there.
        let bindings = crate::app::accelerator_bindings();
        let mut offered: Vec<String> = Vec::new();
        let mut ours: Option<gtk::ShortcutController> = None;
        let controllers = card.observe_controllers();
        for i in 0..controllers.n_items() {
            let Some(controller) = controllers
                .item(i)
                .and_downcast::<gtk::ShortcutController>()
            else {
                continue;
            };
            let mut names: Vec<String> = Vec::new();
            for j in 0..controller.n_items() {
                let Some(shortcut) = controller.item(j).and_downcast::<gtk::Shortcut>() else {
                    continue;
                };
                if let Some(named) = shortcut.action().and_downcast::<gtk::NamedAction>() {
                    let name = named.action_name().to_string();
                    // Membership in the app's own table, not a `win.` prefix: the table
                    // carries `app.` actions too, and GTK's own popover controllers carry
                    // named actions of their own that must not be mistaken for ours.
                    if bindings.iter().any(|(action, _)| *action == name) {
                        names.push(name);
                    }
                }
            }
            if !names.is_empty() {
                assert!(
                    ours.is_none(),
                    "one controller re-offers the application accelerators, not several — \
                     two copies would fire an action twice per keypress"
                );
                offered = names;
                ours = Some(controller);
            }
        }
        let controller = ours.expect("the card installs an application-accelerator controller");
        assert_eq!(
            controller.propagation_phase(),
            gtk::PropagationPhase::Bubble,
            "BUBBLE, so a focused comment entry keeps the keys it binds itself — a \
             capture-phase copy would re-import the window-accel-beats-GtkText hole \
             (ScrAP-137) inside the card"
        );
        for (action, accel) in bindings {
            // A detailed name (`win.format::bold`) is bound as a whole by GtkNamedAction.
            assert!(
                offered.contains(&action),
                "the card must re-offer {action} ({accel}); it is bound app-wide, so \
                 while the card holds focus it would otherwise simply stop working"
            );
        }
        win.destroy();
    }
}
