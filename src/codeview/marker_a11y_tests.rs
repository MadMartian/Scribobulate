//! Live-display tests for the annotation **accessibility** path
//! (`open_stepped_marker_popover`). Excluded from the default `cargo test` (they need a
//! real display and a real paint); run with `cargo test --features
//! gtk-integration-tests`, under Xvfb if headless.
//!
//! These exist because the interesting half of that method is unreachable from a pure
//! test: `marker_hitboxes` is repopulated only by `snapshot_layer`, so nothing about
//! the scroll→await-paint→open sequence can be exercised without an actual frame.

use super::markernav::{CardFocus, Deadline, PendingMarkerOpen};
use super::{AnnotationEdit, CodePreviewView};
use crate::annotations::Direction;
use gtk::prelude::*;
use std::time::Duration;

/// Two annotations, far enough apart that the second starts off-screen in a short
/// window — so the scroll-then-await-paint branch (not just the fast path) runs.
const MD: &str = "Intro paragraph here.\n\n\
    The {==first claim==}{>>first note<<} sits near the top.\n\n\
    filler\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\n\
    filler\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\n\
    The {==second claim==}{>>second note<<} sits far below.\n\n\
    filler\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\n\
    filler\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\nfiller\n\n\
    filler\n\nfiller\n\nfiller\n\nfiller\n\n";

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

/// Pump the main loop until `done` reports true, panicking — and naming `what` — if
/// it never does. Every condition waited on in this module is frame-clock driven:
/// `marker_hitboxes` and the pending-open dispatch are both repopulated by
/// `snapshot_layer`, i.e. by a real PAINT, so none of them can be reached by a single
/// drain of the already-pending events. Hence `Clock::Frame`.
///
/// **Why there is no budget parameter.** This helper used to take one, counted in
/// turns of `MainContext::iteration`, and the M31 migration onto `crate::testpump`
/// converted those turns into a millisecond deadline by picking a multiplier
/// (`budget * 4ms`). That conversion is not merely imprecise, it is invalid, and
/// GTK4Rs/AP-261 says why: turns are not time. The macOS seat MEASURED the same 200
/// turns of `iteration(false)` costing **74 ms** with one live toplevel and **610 ms**
/// with three more alive — an ~8x spread that moves with machine load and with how
/// much other work the loop happens to have to dispatch. No constant converts one
/// into the other, so every per-call-site number inherited from the turn era was a
/// guess wearing a unit, and two seats hit real failures from exactly that.
///
/// What replaces it is not a better number but a different shape: the wait ends when
/// `done()` observes the real state, and `Clock::Frame`'s generous default deadline
/// in `crate::testpump` is a FAILURE bound (GTK4Rs/AP-122), never the completion
/// signal itself. A call site that genuinely tolerates non-convergence keeps a bound
/// of its own — but states it as a duration measured against the mechanism it has to
/// outlast (see [`FIRST_PAINT_DRAIN`], [`FOCUS_TAKE_IDLE_DRAIN`] and
/// [`PENDING_OPEN_DISPATCH_WATCH`]), never as a turn count reinterpreted as ms.
fn settle(what: &str, done: impl FnMut() -> bool) {
    crate::testpump::until(crate::testpump::Clock::Frame, what, done);
}

/// How long a test asserting a NO-OP drains the loop before it reads any state.
///
/// Measured against the **first paint** of a freshly presented window: it has to
/// map, allocate and run at least one frame-clock cycle before `snapshot_layer` has
/// had any chance to arm or fire anything, and a drain that ended sooner would let
/// the negative pass vacuously. There is no predicate to converge on here — the whole
/// claim is that nothing happens — so this span IS the wait, not a bound on one. A
/// quarter second is ~15 frames at a 60 Hz clock, many times the single frame
/// strictly required, so a loaded machine cannot shorten it below its intent.
const FIRST_PAINT_DRAIN: Duration = Duration::from_millis(250);

/// How long the Edit-field test waits after clicking Edit before reading the field's
/// horizontal offset. Measured against the switch to the edit page, which is deferred
/// to an idle (GTK4Rs/AP-30) and then needs the popover's layout pass that writes the
/// offset; there is no event marking "nothing more will write it", so this span is
/// the wait. Before the fix the wrong offset was already present at 400 ms.
const EDIT_PAGE_SETTLE: Duration = Duration::from_millis(400);

/// How long the `CardFocus::Leave` negative drains before reading `has_focus()`.
///
/// Measured against the one `glib::idle_add_local_once` that `CardFocus::Take` queues
/// after `card.present()` to run `child_focus` — deferred deliberately, because a
/// same-tick `child_focus` runs before the popover surface is mapped and silently
/// no-ops. The `Leave` path must be shown NOT to do that, so the drain has to outlast
/// a present plus that idle; ending sooner would prove only that the read was early.
/// The same quarter second as [`FIRST_PAINT_DRAIN`], for the same reason: the
/// mechanism needs one frame and one idle turn, and this gives it many of each.
const FOCUS_TAKE_IDLE_DRAIN: Duration = Duration::from_millis(250);

/// How long the expired-request negative watches for a popover that must never open.
///
/// Measured against the dispatch chain a WRONG implementation would open one
/// through: `snapshot_layer` fires a pending open from a PAINT and defers the actual
/// `popup()` to `glib::idle_add_local_once` (GTK4Rs/AP-30), so a frame plus an idle
/// turn is all it needs. Half a second is ~30 frames at 60 Hz on top of that idle —
/// every opportunity to open the popover, and only then does the test conclude it did
/// not. Unlike the two drains above this one CAN stop early, on the very failure it
/// is looking for, which is why it is an `until_or_for` and not a `drain_for`.
const PENDING_OPEN_DISPATCH_WATCH: Duration = Duration::from_millis(500);

fn hitbox_count(view: &CodePreviewView) -> usize {
    use gtk::subclass::prelude::*;
    view.imp().marker_hitboxes.borrow().len()
}

/// Depth-first search for the first descendant of type `T`. Production code must never
/// re-discover built objects by walking the tree (ScrAP-10 — pass the list forward), but a
/// test driving the real UI has no forwarded handle by design: walking is how it stays
/// an honest end-to-end test of what the user's pointer actually reaches.
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

/// **End to end, on the surface that was actually broken.**
///
/// Drives the real marker-chip popover: open it, click Edit, type, press Enter — and
/// assert the annotation was committed. Before the shared `CommentEntry`, this surface
/// wired its commit only to `save.connect_clicked`, so Enter did nothing here while
/// working in both Create cards.
///
/// The `CommentEntry` unit tests prove the wiring; this proves the surface is really
/// wired *through* it. That distinction is the whole lesson — the Enter-commit bug
/// survived because the obvious test clicked Save, and clicking Save passed throughout.
#[gtktest::test]
fn enter_commits_in_the_marker_edit_popover() {
    use gtk::subclass::prelude::*;
    let pane = crate::preview::render(
        None,
        MD,
        None,
        1.0,
        false,
        &crate::fold::FoldState::default(),
    );
    let view = view_of(pane.clone());
    let win = gtk::Window::new();
    win.set_default_size(700, 400);
    win.set_child(Some(&pane));
    win.present();
    // precondition: chips painted
    settle("the margin chips to paint", || hitbox_count(&view) > 0);

    // Edit/Remove controls only appear when a mutation sink is installed.
    let edits: std::rc::Rc<std::cell::RefCell<Vec<AnnotationEdit>>> =
        std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    view.set_annotation_sink({
        let edits = edits.clone();
        std::rc::Rc::new(move |e: AnnotationEdit| edits.borrow_mut().push(e))
    });

    assert!(
        view.open_stepped_marker_popover(0, Direction::Next),
        "the fixture has annotations"
    );
    // precondition: the marker popover opened
    settle("the marker popover to open", || {
        view.has_open_marker_popover()
    });
    let popover = view
        .imp()
        .marker_card
        .borrow()
        .clone()
        .expect("the card exists once opened");

    let edit = find_button(&popover, "Edit").expect("the popover offers an Edit button");
    edit.emit_clicked();

    let entry = find_descendant::<sourceview::View>(&popover)
        .expect("clicking Edit swaps the comment label for a field");
    crate::widgets::comment_entry::set_comment_text(&entry, "amended via Enter");

    // The route under test. NOT `save.emit_clicked()` — that passed all along.
    crate::widgets::comment_entry::press_key(&entry, gtk::gdk::Key::Return);

    // The sink is deferred to an idle (GTK4Rs/AP-30: never re-render inside the gesture).
    // Enter in the Edit popover's entry must commit the annotation — it did nothing
    // before the shared CommentEntry, while the Save button worked.
    settle(
        "Enter in the Edit popover's entry to commit the annotation",
        || !edits.borrow().is_empty(),
    );
    match &edits.borrow()[0] {
        AnnotationEdit::EditComment { text, .. } => {
            assert_eq!(
                text, "amended via Enter",
                "Enter must commit the typed text"
            );
        }
        other => panic!("expected an EditComment, got {other:?}"),
    }
    crate::testpump::destroy_after_tooltip_delay(&win, || view.popdown_marker_popover());
}

/// TDD 17.57: the Edit field shows the comment it holds, on every opening of the card.
///
/// The reported defect: Edit on an existing comment showed an apparently EMPTY field
/// about every other time, because the whole text was scrolled off to one side of a
/// field that has no horizontal scrollbar. Measured here before the fix: the first
/// open was correct, and from the second on the field's horizontal offset sat at 573 px
/// against a range of zero. So one opening proves nothing — the card is reopened
/// several times, and every opening is checked.
#[gtktest::test]
fn the_edit_field_opens_unscrolled_however_often_the_card_is_reopened() {
    use gtk::subclass::prelude::*;
    let md = "Intro.\n\nThe {==claim==}{>>an existing comment long enough to wrap \
              over several rows of the comment field<<} here.\n\nfiller\n";
    let pane = crate::preview::render(
        None,
        md,
        None,
        1.0,
        false,
        &crate::fold::FoldState::default(),
    );
    let view = view_of(pane.clone());
    let win = gtk::Window::new();
    win.set_default_size(700, 400);
    win.set_child(Some(&pane));
    win.present();
    settle("the margin chips to paint", || hitbox_count(&view) > 0);
    // Edit is offered only when a mutation sink is installed.
    view.set_annotation_sink(std::rc::Rc::new(|_e: AnnotationEdit| {}));
    for opening in 1..=4 {
        assert!(view.open_stepped_marker_popover(0, Direction::Next));
        settle("the marker popover to open", || {
            view.has_open_marker_popover()
        });
        let popover = view
            .imp()
            .marker_card
            .borrow()
            .clone()
            .expect("the card exists once opened");
        find_button(&popover, "Edit")
            .expect("the card offers Edit")
            .emit_clicked();
        let field = find_descendant::<sourceview::View>(&popover)
            .expect("the edit page holds the comment field");
        // No predicate marks "the field has settled at its offset": the wrong offset
        // is written during the page switch's layout, so the read waits out the
        // deferred switch plus several frames of layout and then looks.
        crate::testpump::drain_for(crate::testpump::Clock::Frame, EDIT_PAGE_SETTLE);
        assert!(
            field.is_mapped(),
            "opening {opening}: the edit page is showing"
        );
        assert_eq!(
            field.visible_rect().x(),
            0,
            "opening {opening}: the comment field is scrolled sideways, so its text \
             is off screen and the field looks empty"
        );
        view.popdown_marker_popover();
        settle("the marker popover to close", || {
            !view.has_open_marker_popover()
        });
    }
    crate::testpump::destroy_after_tooltip_delay(&win, || view.popdown_marker_popover());
}

/// A presented window over `md`, with a mutation sink so the card offers Edit.
fn edit_card_window(md: &str) -> (gtk::Window, CodePreviewView) {
    let pane = crate::preview::render(
        None,
        md,
        None,
        1.0,
        false,
        &crate::fold::FoldState::default(),
    );
    let view = view_of(pane.clone());
    let win = gtk::Window::new();
    win.set_default_size(700, 400);
    win.set_child(Some(&pane));
    win.present();
    settle("the margin chips to paint", || hitbox_count(&view) > 0);
    view.set_annotation_sink(std::rc::Rc::new(|_e: AnnotationEdit| {}));
    (win, view)
}

/// Open the first annotation's card, click Edit, and wait out the page switch.
fn open_edit_field(view: &CodePreviewView) -> sourceview::View {
    use gtk::subclass::prelude::*;
    assert!(view.open_stepped_marker_popover(0, Direction::Next));
    settle("the marker popover to open", || {
        view.has_open_marker_popover()
    });
    let card = view
        .imp()
        .marker_card
        .borrow()
        .clone()
        .expect("the card exists once opened");
    find_button(&card, "Edit")
        .expect("the card offers Edit")
        .emit_clicked();
    let field =
        find_descendant::<sourceview::View>(&card).expect("the edit page holds the comment field");
    crate::testpump::drain_for(crate::testpump::Clock::Frame, EDIT_PAGE_SETTLE);
    assert!(field.is_mapped(), "the edit page is showing");
    field
}

/// TDD 17.57, the vertical axis: a comment that fits the field opens with its first
/// line at the top, not scrolled down past it. Measured before the fix: 21 px down on
/// the first opening, leaving only the bottom of the line in view.
#[gtktest::test]
fn the_edit_field_opens_with_a_short_comment_at_its_top() {
    let (win, view) = edit_card_window("Intro.\n\nThe {==claim==}{>>short<<} here.\n");
    for opening in 1..=3 {
        let field = open_edit_field(&view);
        // The scroll value, not `visible_rect().y()`: that reads value − top_margin
        // (GTK4Rs/AP-321), so an unscrolled field reports minus its margin.
        let vadj = field.vadjustment().expect("the field scrolls vertically");
        assert_eq!(
            vadj.value(),
            vadj.lower(),
            "opening {opening}: a one-line comment is scrolled down inside its field, \
             so only the bottom of its line shows"
        );
        view.popdown_marker_popover();
        settle("the marker popover to close", || {
            !view.has_open_marker_popover()
        });
    }
    crate::testpump::destroy_after_tooltip_delay(&win, || view.popdown_marker_popover());
}

/// TDD 17.57, the other half: a comment taller than the field opens scrolled to its
/// end, where the caret is. The short-comment fix defers that scroll past the field's
/// first paint; this proves the deferred scroll still runs.
#[gtktest::test]
fn the_edit_field_opens_a_long_comment_at_its_caret() {
    let long = "word ".repeat(400);
    let (win, view) = edit_card_window(&format!("The {{==claim==}}{{>>{long}end<<}}.\n"));
    let field = open_edit_field(&view);
    let vadj = field.vadjustment().expect("the field scrolls vertically");
    // precondition: the comment really is taller than the field
    assert!(
        vadj.upper() > vadj.page_size() * 2.0,
        "fixture: the comment must overflow the field ({} of {})",
        vadj.upper(),
        vadj.page_size()
    );
    assert!(
        // scroll_mark_onscreen brings the caret's line in, not the margin below it.
        vadj.value() + vadj.page_size() >= vadj.upper() - f64::from(field.bottom_margin()) - 1.0,
        "a long comment must open scrolled to its end, where the caret is \
         (value {} page {} upper {})",
        vadj.value(),
        vadj.page_size(),
        vadj.upper()
    );
    crate::testpump::destroy_after_tooltip_delay(&win, || view.popdown_marker_popover());
}

/// The precondition every other a11y assertion rests on: a presented window paints
/// the margin chips, so `marker_hitboxes` is populated. If this fails, no popover
/// can ever open and the whole keyboard path is dead — which is exactly the
/// symptom seen when driving the app by hand (no chip ever observed).
#[gtktest::test]
fn a_presented_preview_paints_marker_hitboxes() {
    let pane = crate::preview::render(
        None,
        MD,
        None,
        1.0,
        false,
        &crate::fold::FoldState::default(),
    );
    let view = view_of(pane.clone());
    let win = gtk::Window::new();
    win.set_default_size(700, 400);
    win.set_child(Some(&pane));
    win.present();

    // A presented preview must paint its margin chips. If `marker_hitboxes` is still
    // empty when this gives up, no popover can ever open and the whole keyboard path
    // is dead — which is exactly the symptom seen when driving the app by hand.
    settle(
        "a presented preview to paint its margin chips into marker_hitboxes",
        || hitbox_count(&view) > 0,
    );
    crate::testpump::destroy_after_tooltip_delay(&win, || view.popdown_marker_popover());
}

/// The fast path: the target chip is already painted, so the popover opens on the
/// spot with no scroll. This is the common case (annotate, then read it back).
#[gtktest::test]
fn next_annotation_opens_the_popover_for_an_on_screen_marker() {
    let pane = crate::preview::render(
        None,
        MD,
        None,
        1.0,
        false,
        &crate::fold::FoldState::default(),
    );
    let view = view_of(pane.clone());
    let win = gtk::Window::new();
    win.set_default_size(700, 400);
    win.set_child(Some(&pane));
    win.present();
    // precondition: chips painted
    settle("the margin chips to paint", || hitbox_count(&view) > 0);

    assert!(
        view.open_stepped_marker_popover(0, Direction::Next),
        "a document WITH annotations must report a target"
    );
    // The first annotation is on screen, so its popover must open with no scroll.
    settle("the on-screen annotation's popover to open", || {
        view.has_open_marker_popover()
    });
    crate::testpump::destroy_after_tooltip_delay(&win, || view.popdown_marker_popover());
}

/// The branch a pure test cannot reach and the pointer path never runs: the target
/// is BELOW the viewport, so there is no hit-box to open against until a scroll and
/// a repaint have happened. A single `idle_add_local_once` here resolves against the
/// PRE-scroll paint and silently finds nothing (observed live: the view scrolled to
/// the annotation and no popover opened) — hence the bounded tick callback. This
/// test fails if that regresses to an idle.
#[gtktest::test]
fn next_annotation_scrolls_to_an_off_screen_marker_and_opens_it() {
    let pane = crate::preview::render(
        None,
        MD,
        None,
        1.0,
        false,
        &crate::fold::FoldState::default(),
    );
    let view = view_of(pane.clone());
    let win = gtk::Window::new();
    win.set_default_size(700, 400);
    win.set_child(Some(&pane));
    win.present();
    // precondition: chips painted
    settle("the margin chips to paint", || hitbox_count(&view) > 0);

    // Target the LAST annotation from a caret just past the first one. In a 400px
    // window the second claim is well below the fold.
    let anchors: Vec<i32> = {
        use gtk::subclass::prelude::*;
        view.imp()
            .markers
            .borrow()
            .iter()
            .map(|m| m.anchor)
            .collect()
    };
    assert_eq!(anchors.len(), 2, "fixture has exactly two annotations");
    let first = anchors.iter().copied().min().expect("two anchors");

    // The precondition MUST be about the target itself, not "some marker is
    // off-screen": if the target were already painted, `open_stepped_marker_popover`
    // would take the fast path, the test would pass, and it would prove nothing
    // about the scroll branch it exists to cover.
    let target = crate::annotations::step_index(&anchors, first, Direction::Next)
        .expect("a second annotation follows");
    assert!(
        view.marker_hitbox_containing(target).is_none(),
        "precondition: the TARGET marker must be off-screen, else this exercises \
         the fast path and the scroll branch stays untested"
    );

    assert!(view.open_stepped_marker_popover(first, Direction::Next));
    // The off-screen annotation must be scrolled to and its popover opened once the
    // paint repopulates the hit-boxes.
    settle(
        "the off-screen annotation to be scrolled to and its popover opened",
        || view.has_open_marker_popover(),
    );
    crate::testpump::destroy_after_tooltip_delay(&win, || view.popdown_marker_popover());
}

/// The **visible give-up**. A navigation must leave the document AT the
/// target whether or not the popover ever opens. The old code's give-up branch did
/// nothing at all: the user pressed the accelerator, no popover appeared, and the
/// view had not moved either — a completely silent no-op. The converge loop's last
/// aim simply stays applied, so this contract holds on the success path and the
/// timeout path alike, which is why it can be asserted without forcing a timeout.
#[gtktest::test]
fn a_navigation_leaves_the_document_scrolled_to_the_target() {
    use gtk::subclass::prelude::*;
    let pane = crate::preview::render(
        None,
        MD,
        None,
        1.0,
        false,
        &crate::fold::FoldState::default(),
    );
    let view = view_of(pane.clone());
    let win = gtk::Window::new();
    win.set_default_size(700, 400);
    win.set_child(Some(&pane));
    win.present();
    // precondition: chips painted
    settle("the margin chips to paint", || hitbox_count(&view) > 0);

    let anchors: Vec<i32> = view
        .imp()
        .markers
        .borrow()
        .iter()
        .map(|m| m.anchor)
        .collect();
    let first = anchors.iter().copied().min().expect("two anchors");
    let target = crate::annotations::step_index(&anchors, first, Direction::Next)
        .expect("a second annotation follows");
    assert!(
        view.marker_hitbox_containing(target).is_none(),
        "precondition: the target must be off-screen, else no scroll is needed"
    );

    assert!(view.open_stepped_marker_popover(first, Direction::Next));
    // precondition: the navigation completed
    settle("the navigation to complete", || {
        view.has_open_marker_popover()
    });

    // Where the target's line actually sits, read AFTER convergence (F4: read before
    // and `line_yrange` hands back a stale/0 y).
    let vadj = view
        .vadjustment()
        .expect("scrollable view has a vadjustment");
    let it = view.buffer().iter_at_offset(anchors[target]);
    let (target_y, _h) = view.line_yrange(&it);
    let max = (vadj.upper() - vadj.page_size()).max(vadj.lower());
    // The fixture must leave a full viewport of content BELOW the target, or this
    // test proves nothing: with `target_y > max` the clamp below pins `want` to
    // `max`, and a view scrolled merely to its own end satisfies the assertion —
    // both sides collapse onto the same bound and a "scroll to end" stub passes.
    // That is precisely how this guard sat green here while failing on two other
    // platforms, so assert the fixture's adequacy LOUDLY rather than letting a
    // degenerate one quietly disarm the check — a guard whose observable is
    // constant across the behaviours it claims to discriminate is not weak, it is
    // vacuous, and it reports success under every implementation.
    assert!(
        (target_y as f64) <= max,
        "DEGENERATE FIXTURE, not a product failure: the target line sits at {target_y} \
         but the view can scroll no further than {max} (upper={}, page_size={}). Add \
         trailing content after the last annotation so the target can reach the top \
         of the viewport; until then this assertion cannot discriminate.",
        vadj.upper(),
        vadj.page_size(),
    );
    let want = (target_y as f64).clamp(vadj.lower(), max);
    assert!(
        (vadj.value() - want).abs() < 4.0,
        "the document must be left scrolled to the target: vadj.value()={} want={}",
        vadj.value(),
        want
    );
    crate::testpump::destroy_after_tooltip_delay(&win, || view.popdown_marker_popover());
}

/// The GTK4Rs/AP-118 re-pin guard must hold the
/// **post-scroll** position, never a pre-scroll snapshot.
///
/// There is one animation slot per adjustment, and `set_value` beats an in-flight
/// animation outright (it calls `end_updating` — gtkadjustment.c:532). So a guard
/// that bracketed the whole navigation, or reused the pre-warm's pre-scroll snapshot,
/// would re-pin the view straight back to where the navigation started — silently
/// undoing it while every other assertion still passed. The plan calls this "the
/// single easiest way to get this wrong".
///
/// This has to be asserted deliberately. The guard only acts on `value-changed`, and
/// this short fixture has no tall table and so no deferred validation scroll to fire
/// one — mutation-checked (GTK4Rs/AP-78): seeding the guard with a pre-scroll `0.0` does NOT
/// fail `a_navigation_leaves_the_document_scrolled_to_the_target`, because there the
/// guard simply never runs. So nudge the adjustment the way a deferred validation
/// scroll would, and assert where the guard puts it back.
#[gtktest::test]
fn the_repin_guard_holds_the_post_scroll_position_not_the_pre_scroll_one() {
    use gtk::subclass::prelude::*;
    let pane = crate::preview::render(
        None,
        MD,
        None,
        1.0,
        false,
        &crate::fold::FoldState::default(),
    );
    let view = view_of(pane.clone());
    let win = gtk::Window::new();
    win.set_default_size(700, 400);
    win.set_child(Some(&pane));
    win.present();
    // precondition: chips painted
    settle("the margin chips to paint", || hitbox_count(&view) > 0);

    let anchors: Vec<i32> = view
        .imp()
        .markers
        .borrow()
        .iter()
        .map(|m| m.anchor)
        .collect();
    let first = anchors.iter().copied().min().expect("two anchors");
    assert!(view.open_stepped_marker_popover(first, Direction::Next));
    // precondition: the navigation completed and armed the guard
    settle("the navigation to complete and arm the guard", || {
        view.has_open_marker_popover()
    });

    let vadj = view
        .vadjustment()
        .expect("scrollable view has a vadjustment");
    let settled = vadj.value();
    assert!(
        settled > 0.0,
        "precondition: the navigation actually scrolled away from the top, else a \
         pre-scroll snapshot and a post-scroll one are indistinguishable"
    );

    // Exactly what GTK4Rs/AP-118's deferred validation scroll does: re-target the adjustment
    // to the top some frames after `popup()`. The guard runs synchronously inside this
    // emission and must put it straight back.
    crate::saferizer::scrollpos::jump(&vadj, 0.0);
    assert!(
        (vadj.value() - settled).abs() < 4.0,
        "the re-pin guard must restore the POST-scroll position ({settled}), not snap \
         back to a pre-scroll one; landed at {}",
        vadj.value()
    );
    crate::testpump::destroy_after_tooltip_delay(&win, || view.popdown_marker_popover());
}

/// The bound is a **wall-clock deadline**, and it is enforced. An
/// already-expired request must be dropped on the next paint even though its chip IS
/// painted and could be opened: expiry wins over a late hit, so a long-dead
/// navigation can never throw a surprise popover at a rect the user has scrolled away
/// from.
///
/// This is also the guard against regressing to a frame count. A past
/// `glib::monotonic_time()` stamp is expired at *every* refresh rate, so this test
/// states the contract in the units that matter. Mutation-checked (GTK4Rs/AP-78): reverting
/// the expiry arm to the old `hit.is_some() || expired` shape — which returns the hit
/// even when expired — fails this on the popover assertion.
#[gtktest::test]
fn an_expired_pending_open_is_discarded_without_opening_a_popover() {
    use gtk::subclass::prelude::*;
    let pane = crate::preview::render(
        None,
        MD,
        None,
        1.0,
        false,
        &crate::fold::FoldState::default(),
    );
    let view = view_of(pane.clone());
    let win = gtk::Window::new();
    win.set_default_size(700, 400);
    win.set_child(Some(&pane));
    win.present();
    // precondition: chips painted
    settle("the margin chips to paint", || hitbox_count(&view) > 0);

    // Marker 0 is on screen and painted, so a hit-box for it exists RIGHT NOW. The
    // only thing that may stop it opening is the expiry — which is exactly what makes
    // this a test of the deadline and not of a missing chip.
    assert!(
        view.marker_hitbox_containing(0).is_some(),
        "precondition: the target's chip is painted, so only expiry can prevent the open"
    );
    view.imp()
        .pending_marker_open
        .replace(Some(PendingMarkerOpen {
            target: 0,
            focus: CardFocus::Take,
            anchor: 0,
            deadline: Deadline::in_us(-1), // already in the past
        }));
    view.queue_draw();

    // Pump for the popover, NOT for the request to clear. The dispatch is a paint
    // followed by an `idle_add_local_once`, so stopping the moment the request clears
    // would assert before that idle has had a chance to run — the negative would pass
    // vacuously. Pumping the full budget on the popover predicate gives a wrong
    // implementation every opportunity to open it, and only then concludes it didn't.
    let opened = crate::testpump::until_or_for(
        crate::testpump::Clock::Frame,
        PENDING_OPEN_DISPATCH_WATCH,
        || view.has_open_marker_popover(),
    );
    assert!(
        !opened,
        "an EXPIRED request must not open a popover even though its chip is painted"
    );
    assert!(
        view.imp().pending_marker_open.borrow().is_none(),
        "an expired request must be discarded by the paint, not left armed forever"
    );
    crate::testpump::destroy_after_tooltip_delay(&win, || view.popdown_marker_popover());
}

/// A document with no annotations reports no target and opens nothing. The action is
/// deliberately always-enabled, so this is the one activation that must no-op
/// cleanly rather than pop an empty popover.
#[gtktest::test]
fn next_annotation_is_a_clean_no_op_without_annotations() {
    let pane = crate::preview::render(
        None,
        "Just prose, no annotations.\n",
        None,
        1.0,
        false,
        &crate::fold::FoldState::default(),
    );
    let view = view_of(pane.clone());
    let win = gtk::Window::new();
    win.set_default_size(700, 400);
    win.set_child(Some(&pane));
    win.present();
    // Nothing here has an observable to converge on — the whole claim is that
    // NOTHING happens — so drain a fixed span that outlasts the first paint, then
    // read.
    crate::testpump::drain_for(crate::testpump::Clock::Frame, FIRST_PAINT_DRAIN);

    assert!(
        !view.open_stepped_marker_popover(0, Direction::Next),
        "no annotations => no target"
    );
    assert!(!view.has_open_marker_popover(), "and nothing may be shown");
    crate::testpump::destroy_after_tooltip_delay(&win, || view.popdown_marker_popover());
}

/// `dispose` must close an open marker card as it takes it off the pane, so no
/// session outlives the view it points into.
///
/// This is the `dispose` half — the last-resort guarantee covering EVERY teardown
/// path. The reload-layer popdown is guarded separately in `window::reload`, whose
/// test deliberately holds a strong ref so `dispose` cannot mask it. Neither test
/// covers the other's fix.
#[gtktest::test]
fn dispose_closes_the_marker_card() {
    use gtk::subclass::prelude::*;
    let pane = crate::preview::render(
        None,
        MD,
        None,
        1.0,
        false,
        &crate::fold::FoldState::default(),
    );
    let view = view_of(pane.clone());
    let win = gtk::Window::new();
    win.set_default_size(700, 400);
    win.set_child(Some(&pane));
    win.present();
    // precondition: chips painted
    settle("the margin chips to paint", || hitbox_count(&view) > 0);
    assert!(
        view.open_stepped_marker_popover(0, Direction::Next),
        "precondition: the fixture has annotations"
    );
    // precondition: the marker popover is open going into teardown
    settle("the marker popover to be open going into teardown", || {
        view.has_open_marker_popover()
    });

    let popover = view
        .imp()
        .marker_card
        .borrow()
        .clone()
        .expect("the card exists once opened");
    let closed = std::rc::Rc::new(std::cell::Cell::new(false));
    crate::widgets::floating_card::FloatingCardExt::connect_closed(&popover, {
        let c = closed.clone();
        move |_| c.set(true)
    });

    // Run out the tooltip timeout the popover's mapping armed while its surface is
    // still alive: the dispose below unrealizes it mid-delay otherwise (GTK4Rs/AP-117).
    crate::testpump::drain_for(
        crate::testpump::Clock::Frame,
        crate::testpump::TOOLTIP_HOVER_DRAIN,
    );
    // Force dispose: detach the pane and drop every strong ref this test holds.
    win.set_child(gtk::Widget::NONE);
    drop(view);
    drop(pane);
    // `dispose` must close the card as it takes it off the pane, so nothing is left
    // holding a session for a view that is gone.
    settle(
        "dispose to close the marker card (its `closed` signal)",
        || closed.get(),
    );

    crate::testpump::destroy_after_tooltip_delay(&win, || ());
}

/// The `open_marker_popover` choke-point crash guard (GTK4Rs/AP-128/GTK4Rs/AP-128).
/// Opening a marker popover ends in `popup()`, which realizes the popover surface;
/// on an UNREALIZED view that surface's parent is NULL → `GDK_IS_SURFACE` assert →
/// SIGSEGV. A view that is rendered but NEVER presented is unrealized, so a direct
/// open here must be a clean no-op — reaching the assertion without aborting IS the
/// pass (mirrors the GTK4Rs/AP-128 scroll-idle crash test). Mutation note: remove the
/// `is_realized()` guard at the top of `open_marker_popover` and this aborts (the
/// `popup()` fires against a NULL parent surface). This is the choke-point half of
/// the fix; the deferred `snapshot_layer` dispatch also weak-captures to de-pin a
/// torn-down view — defense-in-depth, either alone prevents the crash.
#[gtktest::test]
fn open_marker_popover_is_a_no_op_on_an_unrealized_view() {
    let pane = crate::preview::render(
        None,
        MD,
        None,
        1.0,
        false,
        &crate::fold::FoldState::default(),
    );
    let view = view_of(pane);
    assert!(
        !view.is_realized(),
        "precondition: a rendered-but-never-presented view is unrealized"
    );

    // Directly attempt an open, bypassing the deferred dispatch. The guard must
    // return before any `popup()`.
    view.open_marker_popover(&[0], CardFocus::Take);

    assert!(
        !view.has_open_marker_popover(),
        "the is_realized() guard must skip the open (no popup on an unrealized view)"
    );
}

/// **The defect the whole keyboard read path turned on.** Opening an annotation must
/// move the caret to it, because the caret is what the next step is measured from.
///
/// Without it the walk asked "which annotation follows the caret?" while nothing on
/// the keyboard path ever moved the caret — so every press re-opened the FIRST
/// annotation and the reader could reach exactly one comment in the document. Only a
/// mouse click into the text (which moves the caret itself) advanced it, which is
/// why it read as "works with the mouse, frozen on the keyboard".
///
/// Asserted on the CARET, not on the card: the card is the visible symptom, but the
/// caret is the state the walk reads, and it is the state a future change could
/// break while the first press still looks perfect.
#[gtktest::test]
fn opening_an_annotation_moves_the_caret_to_it_so_the_next_step_advances() {
    use gtk::subclass::prelude::*;
    let pane = crate::preview::render(
        None,
        MD,
        None,
        1.0,
        false,
        &crate::fold::FoldState::default(),
    );
    let view = view_of(pane.clone());
    let win = gtk::Window::new();
    win.set_default_size(700, 400);
    win.set_child(Some(&pane));
    win.present();
    // precondition: the first chip must paint
    settle("the first chip to paint", || hitbox_count(&view) > 0);

    let anchors: Vec<i32> = view
        .imp()
        .markers
        .borrow()
        .iter()
        .map(|m| m.anchor)
        .collect();
    assert_eq!(anchors.len(), 2, "fixture has exactly two annotations");
    let caret = |v: &CodePreviewView| v.buffer().iter_at_mark(&v.buffer().get_insert()).offset();
    // Start from a known position rather than whatever the render left behind — the
    // claim under test is about the DELTA the open produces, so the origin has to be
    // pinned or the first assertion could pass by coincidence.
    let buffer = view.buffer();
    buffer.place_cursor(&buffer.start_iter());
    assert_eq!(caret(&view), 0, "precondition: the caret is at the top");

    // Step once from the top: the first annotation opens AND becomes the position.
    assert!(view.open_stepped_marker_popover(caret(&view), Direction::Next));
    settle("the first annotation's popover to open", || {
        view.has_open_marker_popover()
    });
    let first = anchors.iter().copied().min().expect("two anchors");
    assert_eq!(
        caret(&view),
        first,
        "opening an annotation must leave the caret ON it — this is what makes a \
         second press advance instead of re-opening the same comment"
    );

    // Step again FROM THE CARET, exactly as the command does. It must reach the
    // other annotation, not the one already open.
    assert!(view.open_stepped_marker_popover(caret(&view), Direction::Next));
    settle(
        "the second step to move the caret off the first annotation",
        || caret(&view) != first,
    );
    assert_eq!(
        caret(&view),
        anchors.iter().copied().max().expect("two anchors"),
        "the second step must reach the SECOND annotation"
    );

    // And back: Previous returns to the first, so an overshoot costs one press
    // rather than a lap of the document.
    assert!(view.open_stepped_marker_popover(caret(&view), Direction::Previous));
    settle(
        "Previous to bring the caret back to the first annotation",
        || caret(&view) == first,
    );

    crate::testpump::destroy_after_tooltip_delay(&win, || view.popdown_marker_popover());
}

/// `CardFocus::Leave` must leave the keyboard where the caller had it, and
/// `CardFocus::Take` must move it into the card.
///
/// Both directions in one body deliberately: asserting only that `Leave` does not
/// steal focus passes just as well on a build where NOTHING ever focuses the card,
/// which would silently kill Escape and Tab-to-Edit for every pointer user
/// (GTK4Rs/AP-254's lesson applied at authoring time — an assertion that cannot
/// distinguish the two failures is not measuring the thing it names).
#[gtktest::test]
fn a_card_takes_the_focus_only_when_the_opener_asked_for_it() {
    let pane = crate::preview::render(
        None,
        MD,
        None,
        1.0,
        false,
        &crate::fold::FoldState::default(),
    );
    let view = view_of(pane.clone());
    let win = gtk::Window::new();
    win.set_default_size(700, 400);
    // A focusable stand-in for the surface a real caller keeps the focus on (the
    // annotations list). Anything focusable serves: what is under test is whether
    // the card takes focus away, not what held it.
    let elsewhere = gtk::Button::with_label("elsewhere");
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 0);
    box_.append(&elsewhere);
    pane.set_vexpand(true);
    box_.append(&pane);
    win.set_child(Some(&box_));
    win.present();
    // precondition: chips painted
    settle("the margin chips to paint", || hitbox_count(&view) > 0);
    // precondition: the stand-in can hold the focus (a mapped, active toplevel).
    // Deliberately a HARD failure and not a tolerated one: without an active toplevel
    // `has_focus()` never becomes true (GTK4Rs/AP-119), and letting that slide would
    // grade both focus claims below against a window nothing can focus at all.
    settle(
        "the stand-in button to hold the focus (a mapped, active toplevel)",
        || {
            let _ = elsewhere.grab_focus();
            elsewhere.has_focus()
        },
    );

    view.open_marker_popover(&[0], CardFocus::Leave);
    settle("the CardFocus::Leave card to open", || {
        view.has_open_marker_popover()
    });
    // Pump well past the idle the Take path would have used, so this is a real
    // negative and not merely an early read.
    crate::testpump::drain_for(crate::testpump::Clock::Frame, FOCUS_TAKE_IDLE_DRAIN);
    assert!(
        elsewhere.has_focus(),
        "CardFocus::Leave must not move the focus — a viewer row that steals it \
         ends the reader's browse after one row"
    );

    view.popdown_marker_popover();
    view.open_marker_popover(&[1], CardFocus::Take);
    settle("the CardFocus::Take card to open", || {
        view.has_open_marker_popover()
    });
    // CardFocus::Take must move the focus into the card, or Escape and Tab-to-Edit
    // are dead for every pointer user.
    settle("CardFocus::Take to move the focus into the card", || {
        !elsewhere.has_focus()
    });

    crate::testpump::destroy_after_tooltip_delay(&win, || view.popdown_marker_popover());
}
