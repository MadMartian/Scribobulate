//! Opening the annotation card for a marker group, and the persistent card's
//! get-or-create and first-map pre-warm. `open_marker_popover` is the one funnel
//! every route into a card passes through — a chip click, a stepped command, the
//! annotations viewer, the deferred paint dispatch.

use super::card::AnnotationCard;
use super::cardscroll;
use super::markernav::{CardFocus, Deadline, REPIN_GUARD_US};
use super::markerrows::{build_annotation_row, card_close_button};
use super::CodePreviewView;
use gtk::prelude::*;
use gtk::{gdk, glib};

impl CodePreviewView {
    /// The stable identity of the annotation a marker group is anchored to: the
    /// `src_span.start` of its first member — the same key the annotations viewer
    /// navigates by, and the key [`marker_index_for_src`] resolves back to an index.
    ///
    /// The card stores THIS, never the index: `markers` is rebuilt on every render, so an
    /// index is a bet that the document has not changed underneath it (GTK4Rs/AP-74, and the
    /// ScrAP-187 family). An identity that stops resolving is the well-defined "this
    /// annotation is gone" answer, which is exactly when the card should stop pointing.
    ///
    /// [`marker_index_for_src`]: Self::marker_index_for_src
    fn marker_anchor_identity(&self, idxs: &[usize]) -> Option<usize> {
        use gtk::subclass::prelude::*;
        let markers = self.imp().markers.borrow();
        let first = idxs.first()?;
        Some(markers.get(*first)?.source.construct.captured_at().start)
    }

    /// Build and pop up the comment popover for the annotations on one line.
    /// `pub(super)` so the view's own `snapshot_layer` can dispatch a pending open.
    ///
    /// This is also where the reader's **position** becomes the annotation they just
    /// opened: the caret is moved to the group's anchor. Every route in — a chip click,
    /// a stepped command, the viewer, the deferred paint dispatch — passes through here,
    /// so putting it at this one funnel is what makes "where am I in this document's
    /// annotations" a single answer rather than one per surface (ScrAP-9's reflex applied
    /// to position rather than to a command).
    pub(super) fn open_marker_popover(&self, idxs: &[usize], focus: CardFocus) {
        use gtk::subclass::prelude::*;
        // CHOKE-POINT crash guard (GTK4Rs/AP-128/GTK4Rs/AP-128/GTK4Rs/AP-63). Every path that opens
        // a marker popover funnels through here, and it ends in `popup()`, which realizes
        // the popover's surface. On an UNREALIZED view that surface's parent is NULL →
        // `gdk_surface_new_popup` `GDK_IS_SURFACE` assert → NULL deref SIGSEGV
        // (gtkpopover.c:944-947 / gdksurface.c:885). The one deferred caller
        // (`snapshot_layer`'s idle dispatch) already weak-captures to de-pin a torn-down
        // view; this guard is the second, choke-point half — a realized-but-unmapped
        // fresh tab is still realized, so a legitimate open is never wrongly skipped.
        if !self.is_realized() {
            return;
        }
        // Suppress a redundant re-open of the marker that is ALREADY showing. This function
        // rebuilds the popover child via `set_child` below, so a stray re-dispatch of the SAME
        // marker — a snapshot/idle re-fire (this function is dispatched from `snapshot_layer`,
        // mod.rs), or a mis-routed pointer echo landing back on the marker hitbox on Win32/
        // Quartz — would rebuild the read view over whatever is in the popover and WIPE it.
        // Most visibly it wipes Edit's inline entry the instant it appears, so Edit reads as
        // "does nothing" (mac/Windows, cross-platform). Skipping the rebuild here preserves the
        // in-progress state. A switch to a DIFFERENT marker has different `idxs` and still
        // rebuilds; `open_stepped_marker_popover` advances to a strictly-different marker, so the
        // advance/switch navigation is untouched. The card's own `is_open` (never
        // `is_visible`) is the authoritative flag.
        let card = self.marker_card();
        if card.is_open() && card.shows_exactly(idxs) {
            return;
        }
        // The reader is now AT this annotation — record that by moving the caret to the
        // group's anchor, which is what `open_marker_popover_step` reads on the next
        // press. Without it the stepped walk asked "which annotation follows the caret?"
        // while nothing on the keyboard path ever moved the caret, so every press
        // re-opened the same first annotation and the whole a11y read path reached
        // exactly one comment in the document (only a mouse click into the text, which
        // moves the caret itself, could advance it).
        //
        // Read the anchor and DROP the borrow before placing the cursor: `place_cursor`
        // emits `mark-set` synchronously and its handlers reach back into this view, so
        // holding a `markers` borrow across it would abort on a re-entrant borrow
        // (GTK4Rs/AP-61). Collapsing a selection this way cannot dismiss the card we are
        // about to open — a selection that merely becomes empty is explicitly not a
        // supersession (TDD 17.39).
        let caret = {
            let markers = self.imp().markers.borrow();
            idxs.first().and_then(|&i| markers.get(i)).map(|m| m.anchor)
        };
        if let Some(anchor) = caret {
            let buffer = self.buffer();
            buffer.place_cursor(&buffer.iter_at_offset(anchor));
        }
        let markers = self.imp().markers.borrow();
        let sink = self.imp().annotation_sink.borrow().clone();

        // Pop down the selection CREATE popover (Annotate) if it is showing.
        // Table-cell annotation: a table-cell selection auto-shows the create popover (driven by the
        // primary clipboard), and clicking a cell's margin marker does NOT clear that
        // label selection — so without this the create popover and the marker's comment
        // popover would coexist (two popovers over the same table). Opening a marker means
        // the user is acting on an EXISTING annotation, so the create affordance dismisses.
        if let Some(create) = self.imp().overlay_popover.borrow().as_ref() {
            crate::widgets::floating_card::FloatingCardExt::popdown(create);
        }
        // NO `set_pointing_to` here, deliberately, and no rectangle reached this function
        // to supply one. Anchoring is the card's own job and happens inside its `show`
        // vfunc, on the way through every presentation — see `codeview::card`. That is
        // what makes the stale anchor unrepresentable rather than merely fixed: this path used to
        // point the card at a rect captured before the navigation's converge-scroll, and
        // there is now no rect to capture and no call site that could pass a stale one.
        // Rebuild only the CHILD content for THIS marker (GTK unparents the old child on
        // set_child), then re-show the same card instance.
        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 8);
        // Top margin is smaller than the others because the close button below supplies
        // its own visual breathing room; without the reduction the card reads top-heavy.
        vbox.set_margin_top(2);
        vbox.set_margin_bottom(8);
        vbox.set_margin_start(8);
        vbox.set_margin_end(8);
        vbox.set_width_request(280);
        vbox.append(&card_close_button(&card));
        // The rows scroll as one when they outgrow the pane; the close button stays
        // outside that scroller so it is always on the card (see `cardscroll`).
        let rows = gtk::Box::new(gtk::Orientation::Vertical, 8);
        // One wheel transaction for the whole card, so a wheel over any row's text
        // reaches the rows when that text cannot use it (see `cardscroll::RowWheel`).
        let wheel = cardscroll::RowWheel::default();
        for (n, &idx) in idxs.iter().enumerate() {
            let Some(m) = markers.get(idx) else { continue };
            if n > 0 {
                rows.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
            }
            rows.append(&build_annotation_row(m, &card, sink.clone(), &wheel));
        }
        vbox.append(&cardscroll::fit_to_pane(&rows, &wheel));
        crate::widgets::floating_card::FloatingCardExt::set_child(&card, Some(&vbox));

        // Bug B: keep the pane from jerking (and the popover from missing) on a marker
        // click. Two independent scroll triggers, two defenses:
        //  • FIRST-realization (the very first `set_parent`+`popup` of the view-parented
        //    popover forces the view's first full validation) → handled by the one-shot
        //    PRE-WARM in `prewarm_marker_popover` (at load, scroll 0).
        //  • RE-VALIDATION mid-session: an in-place annotation re-render leaves the tall
        //    table's layout dirty; a later `popup()` flushes it → `validate_onscreen` re-
        //    anchors on the first onscreen paragraph — and a table is ONE `GtkTextChildAnchor`
        //    = one paragraph, so when the viewport sits MID-table the "first onscreen
        //    paragraph" is the table's own line (its top) → a jump to the table top
        //    (gtktextview.c:4707/4746 "odd side effect of triggering a scroll").
        // A one-shot idle restore LOSES that race, and `scroll_to_mark` can't restore a
        // mid-table position (the whole table is one mark → it only lands at the table top).
        // So HOLD the saved scroll value across the popup's settle: re-pin it on each
        // `value-changed`. CRITICAL (operator repro: scroll up, scroll back down, THEN click
        // — only the first time): the validation scroll is DEFERRED, firing several frames
        // after `popup()`, so a short quiet-tick disconnect bails BEFORE it fires and misses
        // it. Keep the guard armed for a generous window and disconnect the instant the USER
        // scrolls — distinguished by `user_scrolling`, which every hand-scroll input source
        // sets but a programmatic validation scroll does NOT. Reset it false right before
        // `popup()` so the popup's own validation scroll is (correctly) treated as
        // non-user and re-pinned. `grab_focus()` is a harmless complement (does not scroll,
        // gtktextview.c:5704).
        let saved_scroll = self.vadjustment().map(|a| a.value());
        // Only on `CardFocus::Take`: on `Leave` this would pull focus off the annotations
        // viewer the reader is arrowing through, which is half of the defect `CardFocus`
        // exists to fix — and it is a *complement* to the re-pin guard below, never a
        // precondition of it (it does not scroll, gtktextview.c:5704), so skipping it
        // leaves that guard intact.
        if focus == CardFocus::Take {
            let _ = self.grab_focus();
        }
        self.imp().user_scrolling.set(false);
        // Hand the card what it is showing and — crucially — the annotation IDENTITY it is
        // anchored to, not a position. From here on the card owns its own geometry: it
        // re-derives the chip rectangle whenever it is shown and on every scroll, which is
        // K and M both. `drop(markers)` first: `marker_anchor_identity` borrows the same
        // `RefCell`, and holding a borrow across a re-entrant borrow aborts (GTK4Rs/AP-61).
        drop(markers);
        // No identity means no annotation to point at — nothing to open.
        let Some(anchor_src) = self.marker_anchor_identity(idxs) else {
            return;
        };
        card.adopt(idxs, anchor_src);
        // The navigation this card belongs to. If a NEWER navigation starts (which bumps
        // `nav_generation`), this card's re-pin guard must stop at once — otherwise it
        // keeps forcing the scroll back to THIS card's position and fights the new
        // navigation's converge-scroll over the shared adjustment. That is the second half
        // of the rapid-activation desync (the first being the converge ticks themselves);
        // `user_scrolling` cannot catch it, because a programmatic navigation deliberately
        // sets `user_scrolling = false`. See `nav_generation`.
        let open_gen = self.imp().nav_generation.get();
        // `present()`, not `popup()`: the card arms its scroll-tracking (K) and its
        // outside-click dismissal (L) and then shows itself, which routes through its
        // `show` vfunc and therefore recomputes its anchor (M).
        card.present();
        // Autohide used to move focus into the popover on open (which is why the comment
        // label above is non-selectable); non-autohide does not, so do it explicitly — this
        // restores keyboard access (Tab/Space to Edit/Remove, and Escape via the popover's
        // CAPTURE-phase controller, which only sees the key event if a popover descendant is
        // actually focused, i.e. the popover is in the event's ancestor chain).
        //
        // DEFERRED one tick, deliberately: a same-tick `child_focus` right after `popup()`
        // runs BEFORE the popover's surface is mapped and silently no-ops (real-keyboard
        // confirmed on Quartz, mac) — focus never enters the popover, so Tab/Space AND Escape
        // are dead until the user clicks inside. The idle runs after the surface is mapped, so
        // Edit is focused when the popover first presents. Same idle-defer family as the Edit
        // handler; focusing a button does not scroll, so it doesn't disturb the Bug-B re-pin.
        //
        // Skipped entirely on `CardFocus::Leave`, where the caller's own surface keeps the
        // focus and therefore owns Escape (see `CardFocus`).
        if focus == CardFocus::Take {
            glib::idle_add_local_once(glib::clone!(
                #[weak]
                card,
                move || {
                    let _ = card.child_focus(gtk::DirectionType::TabForward);
                }
            ));
        }
        if let (Some(vadj), Some(saved)) = (self.vadjustment(), saved_scroll) {
            // Re-pin `saved` on each `value-changed` UNLESS the user has taken over scrolling
            // OR a newer navigation has superseded this card.
            // Edge-triggered + self-healing (researcher, gtktextview.c:2890/4897/8340/8395).
            let reentrant = std::rc::Rc::new(std::cell::Cell::new(false));
            let re = reentrant.clone();
            let id = vadj.connect_value_changed(glib::clone!(
                #[weak(rename_to = v)]
                self,
                move |adj| {
                    if re.get() {
                        return;
                    }
                    // Stop re-pinning if the user grabbed the scrollbar/wheel/keys
                    // (`user_scrolling`) OR a newer navigation superseded this card (`open_gen`
                    // stale) — in either case this guard must not fight the current scroll.
                    if v.imp().user_scrolling.get() || v.imp().nav_generation.get() != open_gen {
                        return;
                    }
                    if (adj.value() - saved).abs() > 0.5 {
                        re.set(true);
                        crate::saferizer::scrollpos::jump(adj, saved);
                        re.set(false);
                    }
                }
            ));
            // Disconnect the guard the moment the user scrolls or a newer navigation
            // supersedes it, else after a generous WALL-CLOCK window (`REPIN_GUARD_US`) that
            // safely spans a DEFERRED validation scroll. This was a `ticks >= 48` frame
            // count; see `REPIN_GUARD_US` for why a count is the wrong shape — it is the same
            // defect as the navigation bound, in the opposite direction.
            let hid = std::rc::Rc::new(std::cell::RefCell::new(Some(id)));
            let vadj_t = vadj.clone();
            let weak_t = self.downgrade();
            let guard_deadline = Deadline::in_us(REPIN_GUARD_US);
            self.add_tick_callback(move |_, _| {
                let done = weak_t.upgrade().is_none_or(|v| {
                    v.imp().user_scrolling.get() || v.imp().nav_generation.get() != open_gen
                });
                if done || guard_deadline.expired() {
                    if let Some(id) = hid.borrow_mut().take() {
                        vadj_t.disconnect(id);
                    }
                    glib::ControlFlow::Break
                } else {
                    glib::ControlFlow::Continue
                }
            });
        }
    }

    /// Get or create the single PERSISTENT annotation card. Bug D / GTK4Rs/AP-117: create it
    /// once and only show/hide it — NEVER unparent/destroy per use (an unrealized popover
    /// leaves a stale tooltip timer pointing at a NULL surface → `GDK_IS_SURFACE`
    /// criticals; `GtkPopover` never calls `gtk_tooltip_unset_surface`). Reuse is also what
    /// upstream does — GtkSourceView's assistants are parented once and hidden/shown
    /// repeatedly — so this is the intended lifecycle, not a workaround.
    /// `dispose()` tears the persistent instance down.
    ///
    /// Everything the popover used to be configured with at this call site — non-autohide,
    /// the parent, the position, the Escape controller, the `closed` bookkeeping — now
    /// lives in [`AnnotationCard::new`], along with the anchoring and dismissal this view
    /// used to have to remember to drive. See [`crate::codeview::card`].
    pub(super) fn marker_card(&self) -> AnnotationCard {
        use gtk::subclass::prelude::*;
        self.imp()
            .marker_card
            .borrow_mut()
            .get_or_insert_with(|| AnnotationCard::new(self))
            .clone()
    }

    /// Bug B PRE-WARM (call once, from the view's first `map`, while scroll is at 0):
    /// create + `set_parent` + a single `popup()`/`popdown()` of the persistent marker
    /// popover so the FIRST real marker click is never the first realization. That first
    /// `set_parent`+`popup` forces the view's first full validation of a tall anchored
    /// table (vadjustment change + `validate_onscreen` re-anchor = a one-shot scroll that
    /// also shifts the clicked chip's hitbox out from under the grab, so the click both
    /// scrolls AND sometimes fails to present). Doing it at scroll 0 makes the re-anchor a
    /// no-op (first onscreen paragraph is already the top → no visible jump), and every
    /// later click hits a warm, already-validated popover. Researcher-sourced (GTK 4.6.9).
    pub(crate) fn prewarm_marker_popover(&self) {
        use gtk::subclass::prelude::*;
        // Crash guard (GTK4Rs/AP-128/GTK4Rs/AP-128): pre-warm ends in `popup()`, which
        // realizes the popover surface — fatal on an unrealized view (NULL parent
        // surface). Checked BEFORE the one-shot flag is consumed, so an (unexpected)
        // unrealized call is retried on the next `map`, not silently burned.
        if !self.is_realized() {
            return;
        }
        if self.imp().marker_popover_warmed.replace(true) {
            return; // already warmed
        }
        let card = self.marker_card();
        // NEVER warm over a live card. The pre-warm drives the SAME persistent instance a
        // real annotation session uses, so its `popdown()` would dismiss a card the user
        // already has open — and it runs from a deferred first-`map` idle, so "the user
        // cannot have opened one yet" is an assumption about idle ordering, not a fact.
        // It is not one: a programmatic navigation (`win.next-annotation`, or the
        // annotations viewer) can open a card in the same turn the view first maps, and
        // then this fires and closes it. Caught by the card's own regression test, where
        // the pre-warm idle landed after the open and silently ended the session.
        //
        // Returning AFTER consuming the one-shot flag is deliberate: a card is open, which
        // means the popover has already been realized and popped up — the first-realization
        // validation this exists to absorb has therefore already happened, and there is
        // nothing left to warm.
        if card.is_open() {
            return;
        }
        // A minimal, on-viewport pointing rect + empty child; popup then immediately
        // popdown so it never actually paints a visible frame, but the present→size_allocate
        // →validate cycle (the scroll-causing part) runs now, at scroll 0.
        //
        // Driven through the RAW card API rather than `present()`/`dismiss()` on
        // purpose: this is not a presentation, it is a one-shot realization, and the card
        // has no annotation to anchor to yet. Going through `present()` would arm the
        // scroll and outside-click wiring for a session that does not exist, and mark the
        // card open. The `show` vfunc still runs and still asks for an anchor — it finds
        // none (`anchor_src` is `None`) and simply leaves `pointing_to` as set here, which
        // is exactly the intended no-op.
        use crate::widgets::floating_card::FloatingCardExt;
        card.set_pointing_to(Some(&gdk::Rectangle::new(0, 0, 1, 1)));
        card.popup();
        card.popdown();
    }
}
