//! Height bounds for the annotation card, so a long annotation can never push the card's
//! controls out of reach.
//!
//! The card is a `FloatingCard`: an in-pane overlay child, clipped to the pane it floats
//! over. A long claim or comment used to be a bare wrapping `GtkLabel`, whose minimum
//! height is its whole wrapped text, so a long enough annotation lost the bottom of the
//! card — the Edit / Remove buttons — past the pane's edge, and left nothing to click.
//!
//! Two bounds, at two levels, each answering a different case:
//!
//!  * [`bounded_text`] caps each claim and comment at a few lines and scrolls the rest, so
//!    a row's buttons stay right under its text however long that text is. This is what
//!    keeps the buttons in view without the reader scrolling for them.
//!  * [`fit_to_pane`] wraps the whole list of rows in a scroller with a small minimum, so
//!    when many annotations share one line and their capped rows still outgrow the pane,
//!    the card — which `FloatingCard` never sizes taller than its host — scrolls its rows
//!    rather than being clipped with later rows' buttons past the pane's edge.
//!
//! Lines, not pixels: the caps are measured from the label's own font, so they track
//! the theme and the zoom rather than a hard-coded height.

use gtk::prelude::*;

/// The most lines of the annotated (claim) text a card row shows before scrolling. Kept
/// short: the claim is context — the reader can see it in the document beside the card.
pub(super) const CLAIM_MAX_LINES: i32 = 4;

/// The most lines of a comment a card row shows before scrolling.
pub(super) const COMMENT_MAX_LINES: i32 = 10;

/// `label` inside a vertical scroller that grows with the text up to `max_lines` lines
/// of it, then scrolls. Never scrolls horizontally: the label wraps.
pub(super) fn bounded_text(label: &gtk::Label, max_lines: i32) -> gtk::ScrolledWindow {
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_propagate_natural_height(true);
    scroller.set_max_content_height(line_px(label) * max_lines);
    scroller.set_child(Some(label));
    scroller
}

/// `body` inside a vertical scroller whose natural height is the body's and whose
/// minimum is small: natural size where the pane has room, scrolling where the card is
/// given less (GTK4Rs/AP-86's wrapper shape).
pub(super) fn fit_to_pane(body: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_propagate_natural_width(true);
    scroller.set_propagate_natural_height(true);
    // An explicit viewport so its vertical scroll policy can be NATURAL. The default
    // (MINIMUM) lays the rows out at their minimum height, and a capped text area's
    // minimum is a sliver, so every comment shrank to a few lines once the rows scrolled
    // (GTK4Rs/AP-358).
    let viewport = gtk::Viewport::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    viewport.set_vscroll_policy(gtk::ScrollablePolicy::Natural);
    viewport.set_child(Some(body));
    scroller.set_child(Some(&viewport));
    scroller
}

/// One line of `label`'s text, in pixels, in the font it will be drawn in.
fn line_px(label: &gtk::Label) -> i32 {
    let (_, height) = label.create_pango_layout(Some("0")).pixel_size();
    height.max(1)
}

/// Live-display tests for the card's height bounds. Run with `cargo test --features
/// gtk-integration-tests`, under Xvfb if headless.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;
    use crate::annotations::Direction;
    use crate::codeview::card::AnnotationCard;
    use crate::codeview::CodePreviewView;
    use gtk::subclass::prelude::*;

    /// The first descendant `GtkButton` labelled `label`.
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

    /// How many descendant `GtkButton`s are labelled `label`.
    fn count_buttons(root: &impl IsA<gtk::Widget>, label: &str) -> usize {
        let mut count = 0;
        let mut child = root.as_ref().first_child();
        while let Some(c) = child {
            let hit = c
                .clone()
                .downcast::<gtk::Button>()
                .is_ok_and(|b| b.label().as_deref() == Some(label));
            count += usize::from(hit) + count_buttons(&c, label);
            child = c.next_sibling();
        }
        count
    }

    /// Every descendant capped scroller that has text to scroll yet is shorter than its
    /// cap, as `(height, cap)`. A capped area with more text than fits should be exactly
    /// as tall as its cap; shorter means its container laid it out at its minimum.
    fn squeezed_scrollers(root: &impl IsA<gtk::Widget>) -> Vec<(i32, i32)> {
        let mut found = Vec::new();
        let mut child = root.as_ref().first_child();
        while let Some(c) = child {
            if let Ok(sw) = c.clone().downcast::<gtk::ScrolledWindow>() {
                let adj = sw.vadjustment();
                let cap = sw.max_content_height();
                if cap > 0 && adj.upper() > adj.page_size() + 1.0 && sw.height() + 1 < cap {
                    found.push((sw.height(), cap));
                }
            }
            found.extend(squeezed_scrollers(&c));
            child = c.next_sibling();
        }
        found
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

    /// A presented 700-wide window `height` tall rendering `md`, with the card open on
    /// its first annotation. Returns `(window, card)`; keep the window alive for the test.
    fn open_card_on(md: &str, height: i32) -> (gtk::Window, AnnotationCard) {
        let pane = crate::preview::render(
            None,
            md,
            None,
            1.0,
            false,
            &crate::fold::FoldState::default(),
        );
        let view = view_of(pane.clone());
        view.set_annotation_sink(std::rc::Rc::new(|_| {}));
        let win = gtk::Window::new();
        win.set_default_size(700, height);
        win.set_child(Some(&pane));
        win.present();
        crate::testpump::until(crate::testpump::Clock::Frame, "the chip to paint", || {
            !view.imp().marker_hitboxes.borrow().is_empty()
        });
        assert!(
            view.open_stepped_marker_popover(0, Direction::Next),
            "precondition: the fixture has an annotation"
        );
        let card = || view.imp().marker_card.borrow().clone();
        crate::testpump::until(crate::testpump::Clock::Frame, "the card to lay out", || {
            card().is_some_and(|c| c.is_mapped() && c.height() > 0)
        });
        (win, card().expect("the card is open"))
    }

    /// The card's height and its pane's, once its first Edit button is allocated.
    fn card_and_pane_heights(card: &AnnotationCard) -> (i32, i32) {
        let edit = find_button(card, "Edit").expect("the card offers Edit");
        crate::testpump::until(
            crate::testpump::Clock::Frame,
            "Edit to be allocated",
            || edit.is_mapped() && edit.height() > 0,
        );
        let bounds = edit
            .compute_bounds(card)
            .expect("Edit shares the card's tree");
        assert!(
            bounds.y() >= 0.0 && bounds.y() + bounds.height() <= card.height() as f32,
            "Edit at y {}..{} lies outside the card's height {}",
            bounds.y(),
            bounds.y() + bounds.height(),
            card.height()
        );
        let pane = card.parent().expect("the card floats over its pane");
        (card.height(), pane.height())
    }

    /// **A long annotation keeps its Edit / Remove buttons on the card** (TDD 17.60).
    ///
    /// Claim and comment together are thousands of pixels tall when wrapped at the card's
    /// width — taller than any pane — so without the bounds the pane clips the card and
    /// the buttons, below the text, are allocated outside it. Asserted on allocations:
    /// the button must lie inside the card, and the card inside its pane.
    #[gtktest::test]
    fn a_long_annotation_keeps_its_buttons_on_the_card() {
        // The claim is long enough to overflow the card but short enough that its chip
        // stays inside the window (a chip off the viewport hides the card, TDD 17.48);
        // the comment is drawn only on the card, so it can be as long as it likes.
        let claim = "word ".repeat(300);
        let comment = "word ".repeat(1500);
        let (win, card) = open_card_on(
            &format!("Intro.\n\nThe {{=={claim}==}}{{>>{comment}<<}} ends.\n"),
            900,
        );
        let (card_h, pane_h) = card_and_pane_heights(&card);
        assert!(
            card_h <= pane_h,
            "the card ({card_h} px) is taller than its pane ({pane_h} px)"
        );
        crate::testpump::destroy_after_tooltip_delay(&win, || card.dismiss());
    }

    /// **Many long annotations on one line still fit a short pane** (TDD 17.60).
    ///
    /// Annotations on one line share one card, one row each. Every row is capped, but
    /// enough of them still add up to more than the pane; the card must scroll its rows
    /// inside the pane rather than be clipped by it, which left the later rows' Edit and
    /// Remove past the pane's edge with nothing to scroll. A short window, because the
    /// defect needs the rows to outgrow the pane but not the monitor.
    ///
    /// Mutation check: sizing the card at its natural height in `size_within` fails the
    /// pane assertion here.
    #[gtktest::test]
    fn many_long_annotations_on_one_line_still_fit_a_short_pane() {
        let comment = "word ".repeat(400);
        let line: String = (0..8)
            .map(|n| format!("{{==claim {n}==}}{{>>{comment}<<}} "))
            .collect();
        let (win, card) = open_card_on(&format!("Intro.\n\n{line}\n"), 420);
        let rows = count_buttons(&card, "Remove");
        assert!(
            rows >= 2,
            "precondition: the line's annotations share one card (found {rows} rows)"
        );
        let (card_h, pane_h) = card_and_pane_heights(&card);
        let squeezed = squeezed_scrollers(&card);
        assert!(
            squeezed.is_empty(),
            "scrolling text areas shorter than their cap (height, cap): {squeezed:?}"
        );
        assert!(
            card_h <= pane_h,
            "the card ({card_h} px) does not fit its pane ({pane_h} px)"
        );
        let rows_scroller = rows_scroller(&card);
        let vadj = rows_scroller.vadjustment();
        assert!(
            vadj.upper() > vadj.page_size(),
            "the rows must scroll inside the capped card: upper {} page {}",
            vadj.upper(),
            vadj.page_size()
        );
        crate::testpump::destroy_after_tooltip_delay(&win, || card.dismiss());
    }

    /// The scroller [`fit_to_pane`] wraps the rows in: the one whose child is a viewport.
    fn rows_scroller(card: &AnnotationCard) -> gtk::ScrolledWindow {
        let mut stack = vec![card.clone().upcast::<gtk::Widget>()];
        while let Some(w) = stack.pop() {
            if let Ok(sw) = w.clone().downcast::<gtk::ScrolledWindow>() {
                if sw
                    .child()
                    .and_downcast::<gtk::Viewport>()
                    .is_some_and(|v| v.vscroll_policy() == gtk::ScrollablePolicy::Natural)
                {
                    return sw;
                }
            }
            let mut c = w.first_child();
            while let Some(child) = c {
                c = child.next_sibling();
                stack.push(child);
            }
        }
        panic!("the card wraps its rows in fit_to_pane's scroller");
    }
}
