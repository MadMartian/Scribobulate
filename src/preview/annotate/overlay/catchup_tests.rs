//! The action card's catch-up when a hidden preview is shown again.
//!
//! In its own file because `overlay.rs` is past the soft line limit.

use super::action_card_of;
use crate::codeview::CodePreviewView;
use crate::testpump::{destroy_after_tooltip_delay, drain_for, until, Clock};
use crate::widgets::floating_card::FloatingCardExt;
use gtk::prelude::*;
use std::time::Duration;

/// Body text only. With a table, the cells are the window's first focusable widgets, and
/// a focus move into one on the way back selects all of it (GTK4Rs/AP-107): its PRIMARY
/// claim clears the body selection (GTK4Rs/AP-120), and there is nothing left for the
/// card to rise over.
const BODY_MD: &str = "Body prose to select, and some more of it after that.\n";
const TABLE_MD: &str = "Body prose before the table.\n\n\
    | Name | Value |\n|------|-------|\n| alpha cell | beta cell |\n";

/// The page that stands in for another tab: showing it unmaps the preview.
const ELSEWHERE: &str = "elsewhere";
const PREVIEW: &str = "preview";

/// A preview as one page of a stack, so a test can hide it and show it again the way a
/// tab switch does.
struct Rig {
    win: gtk::Window,
    stack: gtk::Stack,
    view: CodePreviewView,
    card: crate::widgets::floating_card::FloatingCard,
}

impl Rig {
    fn present(md: &str) -> Self {
        let win = gtk::Window::new();
        let pane = crate::preview::render(
            None,
            md,
            None,
            1.0,
            false,
            &crate::fold::FoldState::default(),
        );
        let view = pane
            .downcast_ref::<gtk::Overlay>()
            .expect("preview pane is a GtkOverlay")
            .child()
            .and_downcast::<gtk::ScrolledWindow>()
            .expect("overlay wraps the scroller")
            .child()
            .and_downcast::<CodePreviewView>()
            .expect("scroller holds the CodePreviewView");
        // The action card only shows with an annotation sink (the split view wires one).
        view.set_annotation_sink(std::rc::Rc::new(|_| {}));
        let stack = gtk::Stack::new();
        stack.add_named(&pane, Some(PREVIEW));
        stack.add_named(&gtk::Label::new(Some("another tab")), Some(ELSEWHERE));
        stack.set_visible_child_name(PREVIEW);
        win.set_default_size(700, 400);
        win.set_child(Some(&stack));
        win.present();
        until(Clock::Frame, "the preview to map", || {
            view.is_mapped() && view.height() > 0
        });
        let card = action_card_of(&view).expect("the action card is wired onto the view");
        Self {
            win,
            stack,
            view,
            card,
        }
    }

    /// The table cell that reads `text`, once the table has laid out.
    fn cell(&self, text: &str) -> gtk::Label {
        until(Clock::Frame, "the table's cells to lay out", || {
            crate::preview::scrib_labels(&self.view)
                .is_some_and(|l| l.borrow().iter().any(|c| c.width() > 0))
        });
        crate::preview::scrib_labels(&self.view)
            .expect("the table registered its cells")
            .borrow()
            .iter()
            .find(|l| l.text() == text)
            .cloned()
            .unwrap_or_else(|| panic!("no cell reads {text:?}"))
    }

    /// Hide the preview behind the other page.
    fn hide(&self) {
        self.stack.set_visible_child_name(ELSEWHERE);
        until(Clock::Frame, "the preview to unmap", || {
            !self.view.is_mapped()
        });
    }

    /// Show the preview again and let the catch-up's ~40 ms decision and a paint land.
    fn show_again(&self) {
        self.stack.set_visible_child_name(PREVIEW);
        until(Clock::Frame, "the preview to map again", || {
            self.view.is_mapped()
        });
        drain_for(Clock::Frame, Duration::from_millis(400));
    }

    fn select_body(&self, from: i32, to: i32) {
        let buf = self.view.buffer();
        buf.select_range(&buf.iter_at_offset(from), &buf.iter_at_offset(to));
        until(
            Clock::Frame,
            "the Annotate card to rise over the selection",
            || self.card.is_visible(),
        );
    }

    fn finish(self) {
        let card = self.card.clone();
        destroy_after_tooltip_delay(&self.win, move || card.popdown());
    }
}

/// **Showing a preview again does not raise the Annotate card the reader left down.**
///
/// The catch-up on map exists for a selection lost while the preview was hidden. It used
/// to re-run the whole decision, which pops the card up over any selection still there,
/// so a tab switch back brought back a card the reader had scrolled away. The control at
/// the end is a real selection change after the re-map, which must still raise it, so
/// the test cannot pass on a card that no longer rises at all. Mutation: the map handler
/// owing a raise (calling `schedule` rather than `arm`) fails here.
#[gtktest::test]
fn showing_the_preview_again_leaves_a_dismissed_card_down() {
    let rig = Rig::present(BODY_MD);
    rig.select_body(0, 9);
    // As the scroll-hide leaves it: the selection stays, the card goes.
    rig.card.popdown();
    drain_for(Clock::Frame, Duration::from_millis(200));

    rig.hide();
    rig.show_again();
    assert!(
        rig.view.buffer().has_selection(),
        "precondition: the body selection survived the round trip, or there is nothing \
         for the card to rise over"
    );
    assert!(
        !rig.card.is_visible(),
        "showing the preview again raised the Annotate card over a selection the reader \
         had left it down for"
    );

    rig.select_body(10, 15);
    rig.finish();
}

/// **A cell selection lost while the preview was hidden takes the card down on map.**
///
/// The PRIMARY handler skips an unmapped preview, so a claim made elsewhere that clears
/// this preview's cell selection is not seen until the catch-up runs on map
/// (GTK4Rs/AP-120). Without it the card the selection raised is still up over a cell
/// that no longer has a selection. Mutation: emptying the `connect_map` handler fails
/// here.
#[gtktest::test]
fn a_cell_selection_lost_while_hidden_takes_the_card_down_on_map() {
    let rig = Rig::present(TABLE_MD);
    let cell = rig.cell("alpha cell");
    cell.select_region(0, 5);
    until(
        Clock::Frame,
        "the Annotate card to rise over the cell",
        || rig.card.is_visible(),
    );

    rig.hide();
    // Another owner claims PRIMARY, as a selection in another window does.
    let foreign = gtk::gdk::ContentProvider::for_value(&"elsewhere".to_value());
    rig.view
        .primary_clipboard()
        .set_content(Some(&foreign))
        .expect("PRIMARY accepts a provider");
    until(
        Clock::Frame,
        "the foreign claim to clear the cell's selection",
        || cell.selection_bounds().is_none(),
    );
    drain_for(Clock::Frame, Duration::from_millis(200));
    assert!(
        rig.card.is_visible(),
        "precondition: nothing took the card down while the preview was hidden, so only \
         the catch-up can"
    );

    rig.show_again();
    assert!(
        !rig.card.is_visible(),
        "the card stayed up over a cell whose selection went while the preview was hidden"
    );
    rig.finish();
}
