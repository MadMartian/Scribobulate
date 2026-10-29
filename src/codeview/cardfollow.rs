//! Keeping an open annotation card on its own annotation when the preview is rendered
//! again under it, without the reader scrolling (TDD 17.47, 17.51).
//!
//! # The defect this closes
//!
//! The card ([`AnnotationCard`](super::card::AnnotationCard)) re-derives where it points
//! whenever the view SCROLLS. A render that rebuilds the text in place moves the chips
//! without moving the scroll: the split view's live re-render, which an external reload
//! also takes, a fold, an annotation refresh. Nothing told the card, so it stayed where
//! the chip had been and pointed at whatever text arrived there. MEASURED on the release
//! build under Xvfb: split view, a card open, six paragraphs inserted above the
//! annotation on disk; the chip moved down ~150 px and the card did not move. Preview
//! mode never showed it, because its reload builds a new view and closes the card first.
//!
//! Re-deriving the rectangle alone is not enough, because the card's IDENTITY is also from
//! the render before. It holds the annotation's `src_span.start` and the marker indices it
//! lists, and an edit above the annotation moves both: the start no longer names any
//! marker, so the card would hide though its annotation is still there, and the indices
//! name whichever marker now sits at them.
//!
//! # What happens at each render
//!
//! [`set_markers`](CodePreviewView::set_markers) is the one place every render of an
//! existing view installs its markers, so it is where this runs: [`plan`] before the new
//! list replaces the old, [`apply`] after.
//!
//! - Each annotation the card lists is found in the new markers by its construct: the same
//!   text with the same neighbours, nearest where it was ([`follow_group`]). The card is
//!   re-keyed to the new indices and start, and re-pointed after the next paint.
//! - If any of them is not found (removed, reworded, or edited right beside it), the card
//!   is dismissed, as TDD 17.51 asks: it must not re-anchor itself to whatever annotation
//!   now occupies the place.
//!
//! **Why after the next paint.** The render has just refilled the buffer, so its line
//! heights are not validated and every geometry read lags until a paint validates the
//! visible band (GTK4Rs/AP-142). The card's re-point reads the chip's position, so it runs
//! in the frame clock's `after-paint`, once, and a later render replaces a pending one.
//! A scroll that follows (the split sync re-projecting the editor's position onto the new
//! text) is the card's own scroll tracking, as before.
//!
//! Following rather than closing, because the reader opened the card to read or edit that
//! annotation, and a change elsewhere in the file is not a reason to take it away. An
//! in-progress Edit draft survives too. TDD 17.45 already assumed the card stays open
//! across the split re-render; this makes where it points agree with that.

use super::card::AnnotationCard;
use super::{CodePreviewView, MarkerData};
use crate::docref::AnchoredSpan;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

/// Where each of `old` is among `new`, by construct identity: for each, the nearest
/// (to where it was captured) of the unclaimed `new` spans naming the same construct,
/// ties toward the earlier. `None` when `old` is empty or any member has no match, so a
/// group is followed whole or not at all.
pub(super) fn follow_group(old: &[AnchoredSpan], new: &[&AnchoredSpan]) -> Option<Vec<usize>> {
    if old.is_empty() {
        return None;
    }
    let mut taken = vec![false; new.len()];
    let mut out = Vec::with_capacity(old.len());
    for span in old {
        let want = span.captured_at().start as i128;
        let j = new
            .iter()
            .enumerate()
            .filter(|&(j, cand)| !taken[j] && span.names_same_construct(cand))
            .min_by_key(|&(j, cand)| ((cand.captured_at().start as i128 - want).abs(), j))
            .map(|(j, _)| j)?;
        taken[j] = true;
        out.push(j);
    }
    Some(out)
}

/// What a render owes the view's open card.
pub(super) enum Plan {
    /// No card open: nothing to do.
    None,
    /// The card's annotations are all in the new render, at these indices.
    Follow(Vec<usize>),
    /// At least one of them is not.
    Dismiss,
}

/// Decide, before `new` replaces the view's markers, what the open card owes the render.
pub(super) fn plan(view: &CodePreviewView, new: &[MarkerData]) -> Plan {
    let Some(card) = view.imp().marker_card.borrow().clone() else {
        return Plan::None;
    };
    if !card.is_open() {
        return Plan::None;
    }
    let old: Option<Vec<AnchoredSpan>> = {
        let markers = view.imp().markers.borrow();
        card.shown()
            .iter()
            .map(|&i| markers.get(i).map(|m| m.source.construct.clone()))
            .collect()
    };
    let new: Vec<&AnchoredSpan> = new.iter().map(|m| &m.source.construct).collect();
    match old.and_then(|old| follow_group(&old, &new)) {
        Some(idxs) => Plan::Follow(idxs),
        None => Plan::Dismiss,
    }
}

/// Carry out `plan` once the new markers are installed.
pub(super) fn apply(view: &CodePreviewView, plan: Plan) {
    let Some(card) = view.imp().marker_card.borrow().clone() else {
        return;
    };
    match plan {
        Plan::None => {}
        Plan::Dismiss => card.dismiss(),
        Plan::Follow(idxs) => {
            let Some(start) = idxs.first().and_then(|&i| {
                view.imp()
                    .markers
                    .borrow()
                    .get(i)
                    .map(|m| m.source.construct.captured_at().start)
            }) else {
                card.dismiss();
                return;
            };
            card.adopt(&idxs, start);
            repoint_after_paint(view, &card);
        }
    }
}

/// Re-point `card` in the frame clock's next `after-paint`, replacing any re-point still
/// pending from an earlier render. A view with no frame clock (not mapped) has nothing
/// to paint, so the card is re-pointed now.
fn repoint_after_paint(view: &CodePreviewView, card: &AnnotationCard) {
    cancel_pending(view);
    let Some(clock) = view.frame_clock() else {
        card.present();
        return;
    };
    view.queue_draw();
    // Weak: the clock outlives neither, but a strong capture would keep a torn-down view
    // alive until the next paint (GTK4Rs/AP-63).
    let weak_view = view.downgrade();
    let weak_card = card.downgrade();
    let id = clock.connect_after_paint(move |clock| {
        let Some(view) = weak_view.upgrade() else {
            return;
        };
        if let Some((_, id)) = view.imp().card_follow_paint.borrow_mut().take() {
            clock.disconnect(id);
        }
        // `present` would re-open a card the reader closed since the render.
        if let Some(card) = weak_card.upgrade().filter(|c| c.is_open()) {
            card.present();
        }
    });
    view.imp().card_follow_paint.replace(Some((clock, id)));
}

/// Disconnect a re-point still waiting for its paint. Also run at the view's `dispose`.
pub(super) fn cancel_pending(view: &CodePreviewView) {
    if let Some((clock, id)) = view.imp().card_follow_paint.borrow_mut().take() {
        clock.disconnect(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docref::Ambiguity;

    /// The construct spans of every `{==…<<}` in `src`, in order, captured as the preview
    /// build captures them.
    fn spans(src: &str) -> Vec<AnchoredSpan> {
        let mut out = Vec::new();
        let mut from = 0;
        while let Some(s) = src[from..].find("{==") {
            let start = from + s;
            let end = start + src[start..].find("<<}").unwrap() + 3;
            out.push(
                AnchoredSpan::capture_in_context(src, start..end, Ambiguity::Nearest).unwrap(),
            );
            from = end;
        }
        out
    }

    const DOC: &str = "Intro paragraph, long enough to hold the context window.\n\n\
        The {==first claim==}{>>first note<<} sits here, with words after it.\n\n\
        Then {==second claim==}{>>second note<<} sits later, with words after it.\n";

    fn refs(v: &[AnchoredSpan]) -> Vec<&AnchoredSpan> {
        v.iter().collect()
    }

    #[test]
    fn a_group_follows_its_annotations_past_a_new_one_above_them() {
        let old = spans(DOC);
        let edited = format!("Added {{==new claim==}}{{>>added<<}} at the top.\n\n{DOC}");
        let new = spans(&edited);
        assert_eq!(follow_group(&old[1..2], &refs(&new)), Some(vec![2]));
        assert_eq!(follow_group(&old, &refs(&new)), Some(vec![1, 2]));
    }

    #[test]
    fn a_group_with_a_member_gone_is_not_followed() {
        let old = spans(DOC);
        let new = spans(&DOC.replace("{==second claim==}{>>second note<<}", "second claim"));
        assert_eq!(follow_group(&old[1..2], &refs(&new)), None);
        assert_eq!(follow_group(&old, &refs(&new)), None);
        assert_eq!(follow_group(&old[0..1], &refs(&new)), Some(vec![0]));
    }

    #[test]
    fn a_reworded_annotation_is_not_followed() {
        let old = spans(DOC);
        let new = spans(&DOC.replace("second note", "a different note"));
        assert_eq!(follow_group(&old[1..2], &refs(&new)), None);
    }

    #[test]
    fn an_empty_group_follows_nothing() {
        let new = spans(DOC);
        assert_eq!(follow_group(&[], &refs(&new)), None);
    }

    #[test]
    fn two_copies_of_one_construct_each_follow_their_own() {
        // Identical text AND identical neighbours, so only position tells them apart: each
        // must take the copy nearest where it was, never both the same one.
        let unit = "Padding paragraph that is the same each time round.\n\n\
            Same {==claim==}{>>note<<} words after it, also the same.\n\n";
        let doc = unit.repeat(2);
        let old = spans(&doc);
        let new = spans(&format!("A new first line.\n\n{doc}"));
        assert_eq!(follow_group(&old, &refs(&new)), Some(vec![0, 1]));
        assert_eq!(follow_group(&old[1..2], &refs(&new)), Some(vec![1]));
    }
}

/// Live-display tests: the card follows a render in place. Need a real display and a
/// real paint; run with `cargo test --features gtk-integration-tests`, under Xvfb if
/// headless.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;
    use crate::annotations::Direction;
    use crate::widgets::floating_card::FloatingCardExt;

    /// The annotation sits far enough below the top that an edit above it leaves its
    /// neighbours alone, and close enough that its chip is on screen in a 300 px window.
    const MD: &str = "Intro paragraph here.\n\n\
        The {==first claim==}{>>first note<<} sits near the top.\n\n\
        filler\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\n\
        filler\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\n\
        filler\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\n";

    /// `MD` with two paragraphs above the annotation, one of them carrying an annotation
    /// of its own, so the card's annotation moves down AND its marker index moves from 0
    /// to 1.
    fn md_with_an_annotation_added_above() -> String {
        format!(
            "A new {{==added claim==}}{{>>added note<<}} paragraph.\n\nAnother new one.\n\n{MD}"
        )
    }

    fn scroller_of(pane: &gtk::Widget) -> gtk::ScrolledWindow {
        pane.downcast_ref::<gtk::Overlay>()
            .and_then(|o| o.child())
            .and_then(|c| c.downcast::<gtk::ScrolledWindow>().ok())
            .expect("the preview pane is an overlay over its scroller")
    }

    fn settle(what: &str, done: impl FnMut() -> bool) {
        crate::testpump::until(crate::testpump::Clock::Frame, what, done);
    }

    /// A presented window over a split-style preview, its card open on the annotation.
    fn open_card() -> (
        gtk::Window,
        gtk::ScrolledWindow,
        CodePreviewView,
        AnnotationCard,
    ) {
        let pane = crate::preview::render(
            None,
            MD,
            None,
            1.0,
            false,
            &crate::fold::FoldState::default(),
        );
        let sw = scroller_of(&pane);
        let view = sw
            .child()
            .and_then(|c| c.downcast::<CodePreviewView>().ok())
            .expect("the scroller holds the preview");
        let win = gtk::Window::new();
        win.set_default_size(700, 300);
        win.set_child(Some(&pane));
        win.present();
        settle("the marker chip to be painted", || {
            !view.imp().marker_hitboxes.borrow().is_empty()
        });
        assert!(
            view.open_stepped_marker_popover(0, Direction::Next),
            "precondition: the fixture has an annotation"
        );
        settle("the marker card to open", || view.has_open_marker_popover());
        let card = view
            .imp()
            .marker_card
            .borrow()
            .clone()
            .expect("the card exists once opened");
        settle("the card to be shown beside its chip", || {
            card.is_visible() && card.pointing_to() == view.marker_chip_rect(0)
        });
        (win, sw, view, card)
    }

    fn re_render(sw: &gtk::ScrolledWindow, md: &str) {
        crate::preview::re_render(
            None,
            sw,
            md,
            None,
            1.0,
            false,
            &crate::fold::FoldState::default(),
        );
    }

    /// **A card follows its annotation through a render in place (TDD 17.47).**
    ///
    /// The split view's live re-render (which an external reload takes) rebuilds the
    /// text under an open card without scrolling. Two paragraphs are added above the
    /// annotation, one with an annotation of its own, so the chip moves down and the
    /// card's annotation becomes marker 1. Before the fix the card kept pointing where
    /// the chip had been, listing marker 0 — the ADDED annotation.
    #[gtktest::test]
    fn the_card_follows_its_annotation_through_a_render_in_place() {
        let (win, sw, view, card) = open_card();
        let before = card.pointing_to().expect("an open card points somewhere");
        let vadj = sw.vadjustment();
        let scroll = vadj.value();

        re_render(&sw, &md_with_an_annotation_added_above());

        settle(
            "the card to point at its own chip in the new render",
            || {
                card.is_visible()
                    && view.marker_chip_rect(1).is_some_and(|chip| {
                        card.pointing_to() == Some(chip) && chip.y() != before.y()
                    })
            },
        );
        assert!(
            (vadj.value() - scroll).abs() < f64::EPSILON,
            "precondition: the render did not scroll, so the card's scroll tracking cannot \
             be what moved it"
        );
        assert!(card.is_open(), "the card stays open across the render");
        assert!(
            card.shows_exactly(&[1]),
            "the card lists its own annotation, now marker 1, not the one added above it"
        );
        crate::testpump::destroy_after_tooltip_delay(&win, || view.popdown_marker_popover());
    }

    /// **A card whose annotation a render drops goes away (TDD 17.51).**
    #[gtktest::test]
    fn the_card_goes_away_when_a_render_drops_its_annotation() {
        let (win, sw, view, card) = open_card();

        re_render(
            &sw,
            &MD.replace("{==first claim==}{>>first note<<}", "first claim"),
        );

        settle("the card to close", || {
            !card.is_open() && !card.is_visible()
        });
        crate::testpump::destroy_after_tooltip_delay(&win, || view.popdown_marker_popover());
    }
}
