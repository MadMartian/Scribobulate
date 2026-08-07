//! The editor-pane Annotate card — the editor's in-surface
//! comment-entry card, the sibling of the preview's create overlay
//! (`preview/annotate/overlay.rs`). Unlike the preview, the editor buffer holds the RAW
//! Markdown source, so there is **no copymap/shift indirection**: a selection's char
//! offsets convert directly to source byte offsets ([`create_from_editor_selection`]),
//! and the created annotation is written straight into the editor buffer via the shared
//! [`apply_annotation_edit`](crate::window::apply_annotation_edit) path (which the normal
//! dirty-tracking + live-preview re-render then pick up for free — decision 3).
//!
//! The card is a typing entry, so it CANNOT live in a popover on X11 (GTK4Rs/AP-83); it is an
//! in-surface `GtkOverlay` child of the editor pane's overlay (the SplitView wraps the
//! editor scroller in that overlay for exactly this). There is no selection popover on
//! the editor side — the editor already has its format/caret overlay; Annotate is raised
//! only by the `win.annotate` action (keyboard/menu/toolbar), so this module just builds
//! the card and returns the trigger closure the action calls.
//!
//! Pure GTK wiring (coverage-excluded, like `preview/annotate/overlay.rs`); the bug-prone
//! placement math and the char→byte/block-crossing logic live in the unit-tested
//! `preview/annotate.rs` helpers it reuses.

use crate::codeview::AnnotationEdit;
use crate::preview::annotate::{create_from_editor_selection, position_card};
use crate::widgets::comment_entry::CommentEntry;
use gtk::glib;
use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

/// Build the editor Annotate card as an in-surface child of `overlay` (which wraps the
/// editor scroller) and return the trigger that raises it over the editor's current
/// selection. Wired once per tab in `SplitView::new` (the editor is persistent — never
/// rebuilt), so the trigger survives every mode switch. The returned closure captures
/// only weak widget refs, so storing it on the SplitView creates no reference cycle
/// (ScrAP-60).
pub(crate) fn wire_editor_annotate_card(
    overlay: &gtk::Overlay,
    editor: &sourceview::View,
) -> Rc<dyn Fn()> {
    // The card: a comment entry + Save, hidden until the action raises it. Same shape and
    // CSS (`.annotation-entry`) as the preview card so the two panes look identical.
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    // Use the const, not a literal: this class is shared with the preview card
    // (`preview/annotate/overlay.rs`) purely for styling — a bare literal here would
    // couple the two by string coincidence and let them drift apart silently. (The
    // `win.select-all` standdown no longer keys off this class at all; it gates on the
    // focused widget's TYPE — see `focus_in_text_entry` — which the card's own entry
    // satisfies regardless of this class.)
    bar.add_css_class(crate::window::ANNOTATION_CARD_CLASS);
    bar.set_halign(gtk::Align::Start);
    bar.set_valign(gtk::Align::Start);
    bar.set_visible(false);

    // True while the card is showing; `pending` holds the selection offsets captured when
    // it was raised, so the commit reads the right span even if the caret later moves.
    let entry_open = Rc::new(Cell::new(false));
    let pending: Rc<Cell<(i32, i32)>> = Rc::new(Cell::new((0, 0)));

    let hide: Rc<dyn Fn()> = Rc::new({
        let bar = bar.clone();
        let entry_open = entry_open.clone();
        move || {
            entry_open.set(false);
            bar.set_visible(false);
        }
    });

    // Commit: map the captured selection to a CreateAnnotation over the RAW source and
    // write it into the editor buffer. DEFER the buffer mutation to an idle turn — doing
    // it synchronously inside the Save `clicked` handler mutates (and, in split, re-renders
    // the preview) while the press gesture is still active, breaking active-state
    // accounting up the ancestor chain (GTK4Rs/AP-30). The dirty-tracking + live-preview re-render
    // then pick the edit up through the normal editor path (no explicit refresh needed —
    // the editor buffer's `changed` already drives them).
    //
    // `CommentEntry` hands the text in and wires this to BOTH Enter and Save, so the two
    // routes cannot diverge.
    let card = CommentEntry::new("", {
        let editor = editor.downgrade();
        let pending = pending.clone();
        let hide = hide.clone();
        move |text: &str| {
            let Some(editor) = editor.upgrade() else {
                return;
            };
            let buf = editor.buffer();
            let source = crate::saferizer::BufferText::of(&buf).into_string();
            let (sa, sb) = pending.get();
            let create = create_from_editor_selection(&source, sa, sb, text);
            hide();
            if let Some(create) = create {
                glib::idle_add_local_once(move || {
                    crate::window::apply_annotation_edit(&buf, AnnotationEdit::Create(create));
                });
            }
        }
    });
    card.style_as_card();
    let entry = card.entry.clone();
    bar.append(&card.entry);
    bar.append(&card.save);
    overlay.add_overlay(&bar);
    // Clip the card to the overlay and keep it OUT of the overlay's size measurement — it
    // must never influence the editor's allocated size (same as the preview card).
    overlay.set_clip_overlay(&bar, true);
    overlay.set_measure_overlay(&bar, false);

    // The trigger: capture the editor selection, reveal + position the card, focus it.
    let show: Rc<dyn Fn()> = Rc::new({
        let editor = editor.downgrade();
        let overlay = overlay.downgrade();
        let bar = bar.downgrade();
        let entry = entry.downgrade();
        let entry_open = entry_open.clone();
        let pending = pending.clone();
        move || {
            let (Some(editor), Some(overlay), Some(bar), Some(entry)) = (
                editor.upgrade(),
                overlay.upgrade(),
                bar.upgrade(),
                entry.upgrade(),
            ) else {
                return;
            };
            let Some((a, b)) = editor.buffer().selection_bounds() else {
                return;
            };
            pending.set((a.offset(), b.offset()));
            // Pre-populate with the comments this annotation is about to MERGE, so a
            // destructive merge is visible and user-controlled instead of silent
            // (b4b9d6a, the preview-side sibling of this card). `insert_or_extend_highlight`
            // extends rather than nests: it replaces every intersecting construct — its
            // `{>>comment<<}` included — with the union carrying the comment committed here.
            // Handing it only the newly typed text destroys the reviewer's earlier remarks
            // with no prompt and no trace.
            //
            // `editor_selection_target` is the SAME resolution the commit runs, and
            // `merged_comment_for` shares `intersecting_highlights` with the mutation that
            // does the merging — so the card and the mutation cannot disagree about what
            // will be merged. A second predicate here would be free to drift.
            let buf = editor.buffer();
            let source = crate::saferizer::BufferText::of(&buf).into_string();
            let existing = match crate::preview::annotate::editor_selection_target(
                &source,
                a.offset(),
                b.offset(),
            ) {
                // A point comment inserts a NEW construct rather than replacing any, so it
                // merges nothing and destroys nothing.
                Some(crate::preview::annotate::SelectionTarget::Highlight(range)) => {
                    crate::annotate::merged_comment_for(&source, range).unwrap_or_default()
                }
                _ => String::new(),
            };
            entry.set_text(&existing);
            entry_open.set(true);
            // Show BEFORE positioning — `position_card` measures the card, and a hidden
            // widget measures 0 (GTK4Rs/AP-85); a stale margin also inflates the re-measure
            // (GTK4Rs/AP-87), both handled inside `position_card`.
            bar.set_visible(true);
            position_card(&editor, &overlay, bar.upcast_ref());
            entry.grab_focus();
            // Caret to the END, and AFTER `grab_focus` — order is load-bearing. A focused
            // `GtkText` selects all its text on focus-in (`gtk-entry-select-on-focus`,
            // default TRUE — gtktext.c:3310-3317), which would silently undo a caret set
            // before the grab. With the pre-population above that is not cosmetic: the
            // merged comment would open fully selected, so the user's first keystroke would
            // replace it and the existing comment would be destroyed after all — the data
            // loss walking back in through the door the pre-population opened. Caret-at-end
            // makes typing APPEND: the user is amending, not starting over.
            entry.set_position(-1);
        }
    });

    // Escape dismisses the card and returns focus to the editor (an in-surface overlay
    // child has no popover autohide, so this is wired explicitly).
    card.wire_escape(&bar, {
        let hide = hide.clone();
        let editor = editor.downgrade();
        move || {
            hide();
            if let Some(editor) = editor.upgrade() {
                editor.grab_focus();
            }
        }
    });

    // Focus leaving the whole card (a genuine outside click / focus move — not the
    // entry↔Save hop, which stays inside the bar subtree) dismisses it. On commit `hide()`
    // has already cleared `entry_open`, so the ensuing leave is a no-op (no double hide).
    let focus = gtk::EventControllerFocus::new();
    focus.connect_leave({
        let hide = hide.clone();
        let entry_open = entry_open.clone();
        move |_| {
            if entry_open.get() {
                hide();
            }
        }
    });
    bar.add_controller(focus);

    show
}

/// The editor card's own contract, driven through the real trigger.
///
/// These assert on the CARD, not on the pure helpers it calls: the helper-level guards in
/// `preview/annotate.rs` pass whether or not this card ever asks them anything, so they
/// cannot catch a card that hands the mutation an empty comment. Only driving `show` can.
///
/// `#[gtktest::test]` rather than `#[test]` — a plain `#[test]` calling `gtk::init` panics on
/// the second test in the binary when the thread changes (GTK4Rs/AP-71).
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    /// Build a wired editor card over `source`, select `[a, b)` chars, raise it, and return
    /// the card's entry — the widget the user is about to type into.
    fn raise_card_over(source: &str, a: i32, b: i32) -> gtk::Entry {
        let editor = sourceview::View::new();
        let buf = editor.buffer();
        buf.set_text(source);
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&gtk::Label::new(None)));
        let show = wire_editor_annotate_card(&overlay, &editor);

        buf.select_range(&buf.iter_at_offset(a), &buf.iter_at_offset(b));
        show();

        // Walk the overlay for the card's entry rather than plumbing it out of the
        // builder: the test must see what the user sees.
        let bar = overlay
            .last_child()
            .expect("the card bar is an overlay child");
        bar.first_child()
            .and_downcast::<gtk::Entry>()
            .expect("the card's first child is the comment entry")
    }

    /// **The data loss, at the surface that caused it.** Raising the card over a selection
    /// that overlaps an existing annotation must show that annotation's comment. Before the
    /// fix the card opened EMPTY and handed the mutation only the newly typed text, so
    /// `insert_or_extend_highlight` replaced the intersecting construct — its
    /// `{>>comment<<}` included — and the reviewer's earlier remark was gone with no prompt
    /// and no trace. This guard fails on an empty card, which is exactly the bug.
    #[gtktest::test]
    fn the_card_opens_prepopulated_with_the_comment_it_would_destroy() {
        let entry = raise_card_over("the earth is {==flat==}{>>citation needed<<}", 16, 20);
        assert_eq!(
            entry.text(),
            "citation needed",
            "the card must open showing the comment its commit is about to overwrite"
        );
    }

    /// Multi-overlap: every intersecting comment, ` | `-joined, source order — the decided
    /// semantics, asserted at the card so the editor cannot drift from the preview.
    #[gtktest::test]
    fn the_card_shows_every_comment_a_multi_overlap_would_destroy() {
        let src = "{==alpha==}{>>note A<<} middle {==omega==}{>>note B<<}";
        let entry = raise_card_over(src, 4, (src.chars().count() - 4) as i32);
        assert_eq!(entry.text(), "note A | note B");
    }

    /// A selection that overlaps nothing opens an EMPTY card — the pre-population must not
    /// invent a comment where none is at risk.
    #[gtktest::test]
    fn the_card_opens_empty_when_nothing_would_be_destroyed() {
        let entry = raise_card_over("the earth is flat and round", 13, 17);
        assert_eq!(entry.text(), "", "nothing is merged, so nothing is offered");
    }

    /// Raise the card inside a **mapped** toplevel and return the window and its entry.
    ///
    /// The mapping is the load-bearing part. A focused `GtkText` selects all its text on
    /// focus-in, and `grab_focus` only delivers focus-in inside a mapped toplevel — so a
    /// card wired into an unmapped overlay never runs the select-on-focus at all, and any
    /// assertion about the caret passes vacuously there. Mapping (`present` + pump) is what
    /// makes the hazard observable. A window MANAGER is *not* required: the WM matters for
    /// delivering real keystrokes, not for intra-hierarchy focus, so this guard bites under
    /// a plain `xvfb-run -a` (POLICY's canonical test command) with no WM running —
    /// measured both ways, not assumed.
    fn raise_card_over_mapped(source: &str, a: i32, b: i32) -> (gtk::Window, gtk::Entry) {
        let editor = sourceview::View::new();
        let buf = editor.buffer();
        buf.set_text(source);
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&editor));
        let show = wire_editor_annotate_card(&overlay, &editor);

        let win = gtk::Window::new();
        win.set_default_size(800, 600);
        win.set_child(Some(&overlay));
        win.present();

        // Pump to mapped, bounded by a timeout SOURCE rather than a wall-clock check
        // between iterations — the latter never returns on an idle display (GTK4Rs/AP-79).
        let deadline = std::rc::Rc::new(std::cell::Cell::new(false));
        glib::timeout_add_local_once(std::time::Duration::from_secs(5), {
            let deadline = deadline.clone();
            move || deadline.set(true)
        });
        while !win.is_mapped() && !deadline.get() {
            glib::MainContext::default().iteration(true);
        }
        assert!(
            win.is_mapped(),
            "the toplevel must map for focus-in to fire"
        );

        buf.select_range(&buf.iter_at_offset(a), &buf.iter_at_offset(b));
        show();
        for _ in 0..50 {
            glib::MainContext::default().iteration(false);
        }

        let bar = overlay
            .last_child()
            .expect("the card bar is an overlay child");
        let entry = bar
            .first_child()
            .and_downcast::<gtk::Entry>()
            .expect("the card's first child is the comment entry");
        (win, entry)
    }

    /// **The trap the pre-population opens, guarded.** The merged comment must open
    /// UNSELECTED with the caret at the end, so the user's first keystroke APPENDS.
    ///
    /// A focused `GtkText` selects all its text on focus-in (`gtk-entry-select-on-focus`,
    /// default TRUE — gtktext.c:3310-3317), which silently undoes a caret set BEFORE
    /// `grab_focus`. With the pre-population that is not cosmetic: the merged comment would
    /// open fully selected and the first keystroke would replace it — the data loss walking
    /// straight back in through the door the fix opened, with the pre-population present
    /// and useless. Setting the caret AFTER the grab is what prevents it, and this guard
    /// fails (selection `Some((0, 15))`) if the two are swapped — verified by mutation.
    #[gtktest::test]
    fn the_prepopulated_comment_opens_unselected_with_the_caret_at_the_end() {
        let (win, entry) =
            raise_card_over_mapped("the earth is {==flat==}{>>citation needed<<}", 16, 20);
        assert_eq!(entry.text(), "citation needed");
        assert!(
            entry.selection_bounds().is_none(),
            "the merged comment must not open SELECTED — the first keystroke would erase it"
        );
        assert_eq!(
            entry.position(),
            "citation needed".chars().count() as i32,
            "caret at end → typing appends to the merged comment rather than replacing it"
        );
        win.destroy();
    }

    /// The card's bar must carry `ANNOTATION_CARD_CLASS` — shared with the preview card
    /// for styling, and the handle every test uses to locate the card (including the
    /// readiness probe in `editoractions.rs`'s standdown rubric, which would spin forever
    /// without it).
    ///
    /// **This does NOT pin the `win.select-all` standdown.** It once did: the standdown
    /// walked the ancestor chain for this class, so the class was load-bearing. `7ebec83`
    /// widened it to every text entry by keying on the focused widget's TYPE
    /// (`focus_in_text_entry` → `gtk::Text`), which reaches this card's entry regardless of
    /// any class. The behavioral guard now lives in `editoractions.rs`
    /// (`select_all_stands_down_for_every_text_entry_and_recovers_for_the_editor`), which
    /// asserts the action's enabled state across the card, find, and replace.
    ///
    /// Kept because the class is genuinely still load-bearing for styling and for test
    /// discovery — but retargeted, because a test whose name and message claim to guard
    /// the standdown while asserting only that a CSS class exists is the vacuous guard
    /// this codebase mutation-tests against (GTK4Rs/AP-78).
    #[gtktest::test]
    fn the_card_bar_carries_the_shared_styling_and_test_discovery_class() {
        let (win, entry) = raise_card_over_mapped("the earth is {==flat==}{>>note<<}", 16, 20);
        let mut w: Option<gtk::Widget> = Some(entry.upcast());
        let mut found = false;
        while let Some(cur) = w {
            if cur.has_css_class(crate::window::ANNOTATION_CARD_CLASS) {
                found = true;
                break;
            }
            w = cur.parent();
        }
        assert!(
            found,
            "the editor card's bar must carry ANNOTATION_CARD_CLASS — the preview card \
             shares it for styling, and every test locates the card through it"
        );
        win.destroy();
    }
}
