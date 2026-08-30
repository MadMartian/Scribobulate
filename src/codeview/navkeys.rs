//! The preview pane's document-navigation keys, and keeping them reachable when a
//! table cell holds the focus.
//!
//! A `GtkTextView`'s navigation keys are class key bindings, so they act only if the
//! key event reaches the view. An anchored table cell breaks that: it is a real
//! widget, it takes focus when the reader clicks or tabs into it, and a focused
//! **selectable `GtkLabel`** answers `Home`, `End`, `Ctrl+Home`, `Ctrl+End`, `Left`
//! and `Right` with its own `move-cursor` bindings, consuming them where the reader
//! can see no effect at all — the document simply does not move (ScrAP-264).
//!
//! Measured, GTK 4.6.9 / X11, with capture- and bubble-phase controllers on the view:
//! with a selectable cell label focused those six keys reach the view's **capture**
//! phase and never its bubble phase, and `move-cursor` never fires; the vertical and
//! page keys bubble through normally, and a `GtkLinkButton` cell (the pure-link cell
//! shape) swallows nothing.
//!
//! So the repair is sited exactly where the key was last seen: a **capture-phase**
//! `GtkEventControllerKey` on the view, which by definition runs before any
//! descendant's own bindings. When focus sits on an anchored child it emits the
//! `move-cursor` GTK's binding would have emitted and stops the key; otherwise it
//! proceeds and changes nothing. Both decisions are pure functions in
//! [`crate::keynav`], leaving this closure decision-free, and
//! [`redirect_navigation_key`] is the whole of the behaviour so a test can drive it
//! without synthesising an X11 key event.
//!
//! Wired from `CodePreviewView::new`, the single place a preview view is built, so a
//! later render path cannot acquire a pane without it.
//!
//! **Two widgets under the pane are deliberately NOT redirected, and both are real
//! today rather than defensive.** The annotation card is a `GtkPopover` `set_parent`ed
//! to the view, so everything in it — including the `CommentEntry` the reader types a
//! comment into — is a *descendant of the view in the widget tree*. A redirect gated on
//! the widget tree alone would therefore steal `Home`, `End` and the arrows out of that
//! entry mid-sentence (GTK4Rs/AP-120/GTK4Rs/AP-121, one level down). Two independent
//! gates exclude it: the popover is its own `GtkNative`, and the entry is a
//! `GtkEditable`. That redundancy is recorded rather than trimmed — a single-gate
//! mutation of either one stays green (GTK4Rs/AP-254), so the guard below asserts the
//! *outcome*, and the second gate is what would still hold if an editable were ever
//! anchored directly in the buffer instead.
//!
//! The `GtkEditable` gate is also why this cannot disturb an **input method**: a preedit
//! belongs either to the view itself (excluded — GTK's own bindings are correct there
//! and this proceeds) or to an editable (excluded), so a capture-phase `Stop` can never
//! land on a key that a composition is consuming. Should an editable cell ever become a
//! real shape, the answer is NOT to drop the gate but to give that widget the same
//! treatment GTK gives an entry inside a scrolled view — let it keep the keys it binds
//! and redirect only what it demonstrably ignores.

use crate::codeview::CodePreviewView;
use crate::keynav::{self, FocusSite, Movement};
use gtk::gdk::{Key, ModifierType};
use gtk::glib;
use gtk::prelude::*;

/// Wire `view` so a navigation key reaches the document even when an anchored child
/// holds the focus. Call once, at construction.
pub(crate) fn wire_document_navigation_keys(view: &CodePreviewView) {
    let keys = gtk::EventControllerKey::new();
    // Capture, not bubble: a bubble-phase controller on the view sits *behind* the
    // focused child's own bindings and would never see the swallowed keys — which is
    // the defect itself, measured.
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(glib::clone!(
        #[weak]
        view,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |_, key, _, mods| redirect_navigation_key(&view, key, mods)
    ));
    view.add_controller(keys);
}

/// Perform `key` on `view`'s document if it is a navigation key that an anchored
/// child would otherwise swallow, reporting whether the key was consumed.
///
/// Split out from the controller closure because a synthetic key event is not
/// available to a headless test: the integration test below calls this directly, so
/// everything except GDK's delivery of the event is covered by the suite (and the
/// delivery half is what the probe behind ScrAP-264 measured).
pub(crate) fn redirect_navigation_key(
    view: &CodePreviewView,
    key: Key,
    mods: ModifierType,
) -> glib::Propagation {
    if !focus_site(view).is_an_anchored_child() {
        return glib::Propagation::Proceed;
    }
    let Some(Movement { step, count }) = keynav::document_movement(key, mods) else {
        return glib::Propagation::Proceed;
    };
    // Exactly what GTK's own binding does — including waking the far-scroll re-issue
    // that `farscroll::wire_buffer_ends_scroll` hangs off this same signal, so
    // Ctrl+End from a cell reaches the end of a document still being laid out just as
    // it does from the view (ScrAP-260).
    view.emit_move_cursor(step, count, false);
    glib::Propagation::Stop
}

/// Answer [`FocusSite`]'s four questions from the live widget tree — the one place
/// that translates widgets into the data the rule is decided on.
fn focus_site(view: &CodePreviewView) -> FocusSite {
    let focus = view.root().and_then(|root| RootExt::focus(&root));
    let Some(focus) = focus else {
        return FocusSite {
            is_the_pane: false,
            inside_the_pane: false,
            shares_the_pane_surface: false,
            editable: false,
        };
    };
    let pane: &gtk::Widget = view.upcast_ref();
    FocusSite {
        is_the_pane: &focus == pane,
        inside_the_pane: &focus == pane || focus.is_ancestor(pane),
        shares_the_pane_surface: focus.native() == view.native(),
        editable: focus.is::<gtk::Editable>(),
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    /// A presented preview pane over a document tall enough to scroll, with a
    /// selectable `GtkLabel` anchored in it the way a table cell is.
    fn pane_with_a_cell() -> (
        CodePreviewView,
        gtk::Label,
        gtk::ScrolledWindow,
        gtk::Window,
    ) {
        let view = CodePreviewView::new();
        let buffer = view.buffer();
        let body: String = (0..2_000).map(|i| format!("line {i}\n")).collect();
        buffer.set_text(&body);

        // Anchored the way `renderer::end` anchors a table: a real, selectable label
        // in the buffer, near the top so it is on screen at scroll 0.
        let mut at = buffer.iter_at_line(3).unwrap();
        let anchor = buffer.create_child_anchor(&mut at);
        let cell = gtk::Label::new(Some("a selectable table cell"));
        cell.set_selectable(true);
        view.add_child_at_anchor(&cell, &anchor);

        let scroller = gtk::ScrolledWindow::new();
        scroller.set_child(Some(&view));
        let window = gtk::Window::new();
        window.set_default_size(600, 400);
        window.set_child(Some(&scroller));
        window.present();
        let ctx = glib::MainContext::default();
        for _ in 0..400 {
            ctx.iteration(false);
            if scroller.vadjustment().page_size() > 0.0 && cell.is_mapped() {
                break;
            }
        }
        (view, cell, scroller, window)
    }

    /// Pump until `done()`, or panic naming `what`. `crate::testpump::until` under
    /// `Clock::Idle` (M31), which already uses this module's own 20s default.
    fn pump_until(what: &str, done: impl FnMut() -> bool) {
        crate::testpump::until(crate::testpump::Clock::Idle, what, done);
    }

    /// **Ctrl+Home reaches the top of the document while a table cell holds the
    /// focus.**
    ///
    /// The pre-fix defect, driven at the seam: with the cell focused the view never
    /// saw the key, so the viewport stayed where it was and the reader saw nothing
    /// happen at all. Mutation guard: make `redirect_navigation_key` return
    /// `Proceed` unconditionally and this fails on the unmoved adjustment — the
    /// assertion is on the *document's* position, not on the return value alone.
    #[gtktest::test]
    fn ctrl_home_from_a_focused_table_cell_moves_the_document() {
        let (view, cell, scroller, window) = pane_with_a_cell();
        let adjustment = scroller.vadjustment();
        pump_until("a scrollable range to exist", || {
            adjustment.upper() - adjustment.page_size() > 500.0
        });
        crate::saferizer::scrollpos::jump(&adjustment, 500.0);
        cell.grab_focus();
        assert!(
            !view.has_focus(),
            "precondition: the CELL must hold the focus, not the view — with the view \
             focused GTK's own bindings work and this test proves nothing"
        );

        let handled = redirect_navigation_key(&view, Key::Home, ModifierType::CONTROL_MASK);

        // The document's position first: it is the property the reader sees, and a
        // mutation that leaves the key unredirected fails HERE (on the pump's
        // watchdog) rather than on the return value alone.
        pump_until("the viewport to reach the top", || {
            adjustment.value() <= 0.0
        });
        assert_eq!(
            adjustment.value(),
            0.0,
            "Ctrl+Home with a cell focused must reach the top of the document"
        );
        assert_eq!(
            handled,
            glib::Propagation::Stop,
            "a navigation key answered for the document must be consumed, or the cell \
             acts on it as well"
        );
        window.destroy();
    }

    /// **With the view itself focused nothing is redirected.**
    ///
    /// GTK's own bindings are correct there; consuming the key here would replace a
    /// working path with a parallel one and double-handle every keystroke.
    #[gtktest::test]
    fn a_navigation_key_with_the_view_focused_is_left_to_gtk() {
        let (view, _cell, _scroller, window) = pane_with_a_cell();
        view.grab_focus();
        assert_eq!(
            redirect_navigation_key(&view, Key::Home, ModifierType::CONTROL_MASK),
            glib::Propagation::Proceed
        );
        window.destroy();
    }

    /// **A key that is not document navigation is never consumed**, whatever holds
    /// the focus — a capture-phase controller on the pane sees every keystroke in it,
    /// so stopping one that is not ours would swallow accelerators and typing.
    #[gtktest::test]
    fn a_non_navigation_key_from_a_focused_cell_is_left_alone() {
        let (view, cell, _scroller, window) = pane_with_a_cell();
        cell.grab_focus();
        assert_eq!(
            redirect_navigation_key(&view, Key::a, ModifierType::empty()),
            glib::Propagation::Proceed
        );
        assert_eq!(
            redirect_navigation_key(&view, Key::c, ModifierType::CONTROL_MASK),
            glib::Propagation::Proceed
        );
        window.destroy();
    }

    /// **A key typed into a popover parented to the pane is never redirected.**
    ///
    /// The annotation card is a `GtkPopover` `set_parent`ed to the view, so its
    /// `CommentEntry` is a descendant of the view *in the widget tree* — pressing Home
    /// there is the reader moving their caret in a sentence they are typing, and a
    /// redirect gated on the tree alone would scroll the document instead and leave the
    /// caret behind. This builds the same structural shape (a `set_parent`ed popover
    /// holding an entry) rather than the annotation machinery, because it is the shape,
    /// not the feature, that decides the answer.
    ///
    /// Honest about its guards: TWO independently sufficient gates exclude this — the
    /// popover is its own `GtkNative`, and the entry is a `GtkEditable` — so neutering
    /// either one alone leaves this green (GTK4Rs/AP-254). It asserts the outcome
    /// deliberately; both gates must go for it to fail, and it is the only test that
    /// exercises either against live widgets rather than synthetic booleans.
    #[gtktest::test]
    fn a_key_in_a_popover_parented_to_the_pane_is_left_alone() {
        let (view, _cell, _scroller, window) = pane_with_a_cell();
        let entry = gtk::Entry::new();
        let popover = gtk::Popover::new();
        popover.set_autohide(false);
        popover.set_child(Some(&entry));
        popover.set_parent(&view);
        popover.popup();
        let ctx = glib::MainContext::default();
        for _ in 0..200 {
            ctx.iteration(false);
            if entry.is_mapped() {
                break;
            }
        }
        entry.grab_focus();

        // Precondition: focus really is inside the popover, and that popover really is a
        // descendant of the pane — without both, this passes for the wrong reason.
        let focus = view.root().and_then(|r| RootExt::focus(&r));
        let focus = focus.expect("the entry must hold the focus");
        assert!(
            focus.is_ancestor(view.upcast_ref::<gtk::Widget>()),
            "precondition: a set_parent'd popover's contents must be descendants of the              pane, or this test is not exercising the case it names"
        );

        for (key, mods) in [
            (Key::Home, ModifierType::empty()),
            (Key::End, ModifierType::empty()),
            (Key::Home, ModifierType::CONTROL_MASK),
            (Key::Left, ModifierType::empty()),
        ] {
            assert_eq!(
                redirect_navigation_key(&view, key, mods),
                glib::Propagation::Proceed,
                "a key typed into a popover parented to the pane must reach the widget                  the reader is typing in, not the document ({key:?} {mods:?})"
            );
        }

        // A set_parent'd popover is not auto-unparented (GTK4Rs/AP-80), and popdown
        // before unparent (GTK4Rs/AP-123).
        popover.popdown();
        popover.unparent();
        window.destroy();
    }

    /// **A selection-extending key stays with the cell.** Shift+Home selects the
    /// cell's own text, which is the only selection a table cell can hold.
    #[gtktest::test]
    fn a_shifted_navigation_key_from_a_focused_cell_stays_with_the_cell() {
        let (view, cell, _scroller, window) = pane_with_a_cell();
        cell.grab_focus();
        assert_eq!(
            redirect_navigation_key(&view, Key::Home, ModifierType::SHIFT_MASK),
            glib::Propagation::Proceed
        );
        window.destroy();
    }
}
