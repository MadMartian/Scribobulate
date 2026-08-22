//! What the **editor** puts on a clipboard: plain text, never a `GtkTextBuffer`.
//!
//! Scoped to the editor view deliberately (M46): the preview pane is a plain, un-taken-over
//! `GtkTextView`-family widget for PRIMARY, and stays that way — see "The two clipboards are
//! not the same problem" below for why, and [`wire_middle_click_paste`]'s doc for why that gap
//! does not reopen the defect this module exists to close.
//!
//! # Why this module exists
//!
//! By default `GtkTextView` publishes rich content — a `GtkTextBuffer` carrying
//! GtkSourceView's syntax-highlight tags — to both the CLIPBOARD and, at realize, the
//! PRIMARY selection. Pasting that back into the application does **not** insert a
//! string: `gtk_text_buffer_paste_clipboard` takes a local-value shortcut, hands the
//! buffer straight to `insert_range_not_inside_self`, and that walks the copied region
//! with `gtk_text_iter_forward_to_tag_toggle`, emitting **one `insert-text` per
//! tag-delimited run**.
//!
//! That chunking is the root of an entire defect class. It has no line-terminator
//! awareness whatsoever, so a tag toggling between the `\r` and the `\n` of a CRLF
//! splits the pair across two emissions — and any handler that repairs the payload then
//! sees a chunk ending in what looks exactly like a lone `\r`. ScrAP-312 records the
//! measurements; `probes/` holds the rigs.
//!
//! **Publishing plain text removes the mechanism rather than the symptom.** A string on
//! the clipboard is deserialised into a *fresh, untagged* buffer
//! (`gtk_text_buffer_new(NULL)`), which has no tag toggles, so a paste arrives as
//! exactly ONE emission however the source was highlighted. MEASURED end to end: a
//! source with a toggle inside a CRLF pasted as 2 emissions splitting the pair as a
//! buffer, and as `1 emission, 61 62 0d 0a 63 64` as text. It is also the honest model
//! for this application — a `.md` file cannot represent a syntax-highlight tag, so
//! intra-application rich paste was carrying state the document format has no way to
//! express.
//!
//! # The two clipboards are not the same problem
//!
//! **CLIPBOARD** is written only on an explicit copy or cut, so overriding the two
//! signals is enough, and it *is* covered on both panes: this module does it for the
//! editor ([`wire_plaintext_clipboard`]), and the preview publishes plain text by its
//! own separate route (`preview::interactions::wire_copy_clipboard`, which resolves a
//! selection back to Markdown source — not a copy of this module's mechanism, because
//! the preview's "copy" has its own Markdown-vs-rendered-text decision to make). Every
//! route reaches the editor's two signals: the context menu, the selection bubble and
//! the keybindings all go through `g_signal_emit_by_name`
//! (`gtktextview.c`'s `gtk_text_view_activate_clipboard_*`), and this window's `win.copy`
//! / `win.cut` actions emit them too.
//!
//! **PRIMARY is not taken over on either pane**, and that is a decision, not a gap.
//! GTK republishes it continuously as the selection changes, from an
//! `add_selection_clipboard` call inside `GtkTextView`'s own realize, so a publisher-side
//! fix means displacing that registration with a custom `GdkContentProvider`. Two designs
//! for that were built and measured to destruction (`probes/textview-selection-clipboard.c`,
//! `probes/textview-primary-overwrite.c`): removing GTK's registration is ref-counted and
//! undone by every `set_buffer`, which raises a `selection_clipboard != NULL` critical from
//! inside GTK's own `set_buffer` on the *next* one — and the preview swaps its buffer on
//! every re-render, so that route could never have been extended there even if the editor
//! kept it. Overwriting the content instead is worse: `gtk_text_buffer_content_detach`
//! collapses the selection the instant a foreign provider displaces GTK's, destroying the
//! very selection being published. Both designs are gone from this tree; do not re-attempt
//! them.
//!
//! Instead, [`wire_middle_click_paste`] fixes the **consumer**, on the editor only: it reads
//! PRIMARY as TEXT rather than asking for `GTK_TYPE_TEXT_BUFFER`, which GDK can satisfy from
//! *any* publisher — including an ordinary, un-taken-over `GtkTextView`'s default rich
//! content, which is exactly what the preview still publishes. So a preview selection
//! middle-click-pasted into the editor never reaches ScrAP-312's tag-toggle chunking either,
//! with no takeover needed on the preview's side — proved by
//! `a_preview_selection_pastes_into_the_editor_as_one_plain_text_emission` in this module's
//! tests, alongside `a_preview_selection_still_publishes_gtks_default_rich_content_to_primary`
//! pinning that the preview really does still publish the rich content the fix has to
//! tolerate. What the consumer-side fix does *not* reach is ScrAP-313's per-select-then-
//! deselect `GtkTextBufferContent` leak — that is a GTK-core defect independent of who
//! publishes, present with or without a takeover on either pane, so it is out of scope here
//! and stays recorded there. PRIMARY is **not** an X11-only concern regardless: MEASURED on
//! Quartz, `GtkTextView` publishes to PRIMARY there, the content is a `GtkTextBuffer`, it is
//! readable, and a middle-click paste through it works with a three-button mouse.
//! `probes/primary-liveness.c` and `probes/primary-middleclick.c`.

use gtk::glib;
use gtk::prelude::*;

/// Publish **plain text** on the CLIPBOARD for copy and cut, instead of GTK's default
/// rich `GtkTextBuffer`.
///
/// Wired onto the editor view. The preview pane already publishes plain text by its own
/// route (`preview::interactions::wire_copy_clipboard`, which resolves a selection back
/// to Markdown source), so this closes the remaining half.
///
/// # Why cut needs more than stopping the emission
///
/// `gtk_text_buffer_cut_clipboard` is `begin_user_action` + `cut_or_copy(delete_region
/// _after = TRUE)` + `end_user_action`, and the deletion happens *inside* the path being
/// suppressed. Stopping the emission therefore copies without cutting. The explicit
/// deletion must therefore be grouped with the copy, which is what keeps the whole cut a
/// **single** undo step; without that grouping the deletion becomes its own step and one
/// Ctrl+Z leaves the document half-restored. MEASURED: `"KEEP<CUTME>KEEP"` cuts to
/// `"KEEPKEEP"` with `"<CUTME>"` on the clipboard, and one undo restores the original.
///
/// The grouping is [`crate::window::undo::UndoGroup`], **not** a raw
/// `begin_user_action`/`end_user_action` pair — this paragraph used to prescribe the raw
/// pair, which the code beside it had already stopped using, because `UndoGroup` also
/// flushes the redo-merge barrier a hand-rolled pair omits. A doc that names the rejected
/// implementation is worse than one that names none: it reads as the sanctioned route.
///
/// # Why an empty selection needs no special case here
///
/// `cut_or_copy` returns early when nothing is selected, so an empty payload never
/// reaches this handler — and unlike PRIMARY, CLIPBOARD carries no ownership obligation
/// to release, because it is only ever written by an explicit copy or cut. The
/// `selection_bounds()` test below is what makes that true rather than assumed.
fn wire_plaintext_clipboard(view: &sourceview::View) {
    /// Put the view's current selection on CLIPBOARD as plain text, and report whether
    /// there was one. Copy and cut differ only in what they do AFTER this; spelling it
    /// twice is how the two drift, and the pair is the whole subject of this module.
    fn publish_selection(v: &sourceview::View) -> Option<gtk::TextBuffer> {
        let buf = v.buffer();
        let (start, end) = buf.selection_bounds()?;
        v.clipboard()
            .set_text(crate::saferizer::BufferText::of_range(&buf, &start, &end).as_str());
        Some(buf)
    }

    view.connect_copy_clipboard(|v| {
        publish_selection(v);
        v.stop_signal_emission_by_name("copy-clipboard");
    });

    view.connect_cut_clipboard(|v| {
        if let Some(buf) = publish_selection(v) {
            // `UndoGroup`, not a raw begin/end pair: it also flushes the redo-merge
            // barrier first, which a hand-rolled pair here would silently omit.
            let _grp = crate::window::undo::UndoGroup::new(&buf);
            buf.delete_selection(true, v.is_editable());
        }
        v.stop_signal_emission_by_name("cut-clipboard");
    });
}

/// Give an editor view this application's clipboard behaviour: both halves, together.
///
/// **This is the module's only entry point, and the two halves are deliberately not
/// callable separately.** They are not independent features that happen to be wired
/// side by side — they are the outbound and inbound directions of one contract ("this
/// view moves plain text, never a rich `GtkTextBuffer`"), and a view that got one
/// without the other would satisfy the contract in one direction only, silently. The
/// sibling next door has the same shape for the same reason
/// (`lineendings::new_editor_buffer` / `wire_paste_normalization`), and POLICY
/// § Typed GTK seams' "seal the exit API" rung is the general form: an opt-in pair is a
/// pair someone eventually half-opts into.
pub(crate) fn wire_editor_clipboards(view: &sourceview::View) {
    wire_plaintext_clipboard(view);
    wire_middle_click_paste(view);
}

/// Replace GTK's middle-click PRIMARY paste with a plain-text one, **on this view only**.
///
/// # Why the consumer and not the publisher
///
/// The defect is that a same-application middle-click paste arrives as one `insert-text`
/// emission per syntax-highlight tag toggle, and a toggle landing inside a `\r\n` is what
/// corrupts CRLF (ScrAP-312). GTK causes that by asking PRIMARY for `GTK_TYPE_TEXT_BUFFER`
/// and doing a rich buffer-to-buffer insert; asking for text instead fixes it at the point
/// where it goes wrong.
///
/// Two publisher-side designs were built and measured to destruction first, and both are
/// recorded in `probes/` so they are not re-attempted:
/// `textview-selection-clipboard.c` (removing GTK's selection-clipboard registration is
/// ref-counted, undone by every `set_buffer`, and makes GTK's own removal raise from inside
/// it) and `textview-primary-overwrite.c` (`gtk_text_buffer_content_detach` collapses the
/// selection the instant a foreign provider displaces GTK's).
///
/// # Why this also protects a PRIMARY selection made in the preview pane
///
/// This is wired **only** on the editor view, and PRIMARY is never taken over on the
/// preview (module doc, "The two clipboards are not the same problem") — the preview
/// re-renders (swaps its buffer) too often for the publisher-side designs above to have
/// survived there even if they had survived on the editor. But the read here asks PRIMARY
/// for TEXT, not `GTK_TYPE_TEXT_BUFFER`, and GDK can satisfy a text read from *any*
/// publisher's content, tagged or not — so a middle click landing on this view pastes the
/// same single, untagged emission whether the selection it reads was made in this editor
/// or in an ordinary, un-taken-over preview `GtkTextView`. ScrAP-312's chunking never runs
/// on either source. MEASURED end to end (not just reasoned):
/// `a_preview_selection_pastes_into_the_editor_as_one_plain_text_emission`.
///
/// # Why a CAPTURE-phase gesture that CLAIMS
///
/// MEASURED, `probes/middleclick-primary-paste.m`. GTK's own click gesture is added with no
/// explicit phase, so it runs in BUBBLE. A capture-phase gesture therefore runs first, and
/// claiming the sequence there DENIES GTK's branch — the text arrives once. The same claim
/// in the bubble phase is too late and the text arrives twice.
///
/// This is why `gtk-enable-primary-paste` is **not** touched. That setting is per-display,
/// so turning it off would remove middle-click paste from every `GtkEntry` in the process
/// (seven production sites) to change one view, and it is a real convention on X11.
///
/// Denying button 2 costs nothing else: `GtkTextView` has exactly three button sites, and
/// the only other button-2 behaviour is `released` raising the on-screen keyboard. The drag
/// gesture is `GDK_BUTTON_PRIMARY` and cannot see button 2.
fn wire_middle_click_paste(view: &sourceview::View) {
    let gesture = gtk::GestureClick::new();
    gesture.set_button(gtk::gdk::BUTTON_MIDDLE);
    // Capture, not bubble: see the doc comment. Bubble double-pastes.
    gesture.set_propagation_phase(gtk::PropagationPhase::Capture);

    gesture.connect_pressed(glib::clone!(
        #[weak]
        view,
        move |g, _n, x, y| {
            // Claim FIRST. GTK's bubble-phase gesture is denied by the claim, not by the
            // asynchronous read that follows it, so claiming after the await would let GTK
            // paste as well.
            g.set_state(gtk::EventSequenceState::Claimed);

            if !view.is_editable() {
                return;
            }

            // Resolve the destination ONCE, here, and carry it across the await as a mark —
            // an iterator would not survive the buffer changing underneath it (ScrAP-244).
            let (bx, by) =
                view.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
            let Some(iter) = view.iter_at_location(bx, by) else {
                return;
            };
            let buf = view.buffer();
            let mark = buf.create_mark(None, &iter, true);

            let primary = view.primary_clipboard();
            primary.read_text_async(
                gtk::gio::Cancellable::NONE,
                glib::clone!(
                    #[weak]
                    view,
                    move |res| {
                        let buf = view.buffer();
                        let Ok(Some(text)) = res else {
                            buf.delete_mark(&mark);
                            return;
                        };
                        let mut at = buf.iter_at_mark(&mark);
                        // One group, so the whole paste is one undo step — the property
                        // TDD 1.11 asserts for every other paste route.
                        let _grp = crate::window::undo::UndoGroup::new(&buf);
                        buf.insert(&mut at, &text);
                        buf.delete_mark(&mark);
                    }
                ),
            );
        }
    ));

    view.add_controller(gesture);
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;
    use sourceview::prelude::*;

    fn editor(md: &str) -> (sourceview::Buffer, sourceview::View) {
        let buf = crate::lineendings::new_editor_buffer();
        buf.set_language(
            sourceview::LanguageManager::default()
                .language("markdown")
                .as_ref(),
        );
        buf.set_enable_undo(true);
        buf.set_text(md);
        let view = sourceview::View::with_buffer(&buf);
        view.set_editable(true);
        wire_plaintext_clipboard(&view);
        wire_middle_click_paste(&view);
        (buf, view)
    }

    fn present(view: &impl IsA<gtk::Widget>) -> (gtk::Window, glib::MainContext) {
        let win = gtk::Window::new();
        // A default size, not just mapped: `is_mapped()` alone says nothing about
        // allocation — an empty-buffer view can map at 0x0 and stay there, which
        // makes any point-based hit test (`iter_at_location`) return None forever.
        win.set_default_size(400, 300);
        win.set_child(Some(view));
        win.present();
        crate::testpump::until(crate::testpump::Clock::Idle, "the window to map", || {
            win.is_mapped()
        });
        (win, glib::MainContext::default())
    }

    /// Drain the main loop for a bounded span with no completion predicate of its
    /// own — every call site below just needs the async clipboard read/paste
    /// machinery a chance to run before it inspects the result with its own
    /// predicate loop. `Clock::Idle`: the work waited on (a `read_text_async`
    /// callback, `insert-text` dispatch) is all posted back to this context, not
    /// frame-clock or worker-thread driven. `budget * 5ms` matches this function's
    /// old worst-case ceiling (non-blocking iteration + a 5ms sleep on every turn
    /// that dispatched nothing) so the call sites below keep the same margin.
    fn pump(_ctx: &glib::MainContext, budget: u32) {
        crate::testpump::drain_for(
            crate::testpump::Clock::Idle,
            std::time::Duration::from_millis(budget as u64 * 5),
        );
    }

    fn text_of(buf: &sourceview::Buffer) -> String {
        crate::saferizer::BufferText::of(buf).into_string()
    }

    /// The button-2 `GestureClick` [`wire_middle_click_paste`] installs, if any — the
    /// lookup both the wiring test and the cross-pane paste test below need, kept in
    /// one place so a future change to how the gesture is found (e.g. adding a second
    /// button-2 controller by mistake) cannot silently diverge between them.
    fn find_middle_click_gesture(view: &sourceview::View) -> Option<gtk::GestureClick> {
        let controllers = view.observe_controllers();
        for i in 0..controllers.n_items() {
            let obj = controllers.item(i).expect("controller");
            if let Ok(click) = obj.downcast::<gtk::GestureClick>() {
                if click.button() == gtk::gdk::BUTTON_MIDDLE {
                    return Some(click);
                }
            }
        }
        None
    }

    /// **A same-application copy/paste arrives as exactly ONE `insert-text` emission,
    /// and a CRLF survives it.**
    ///
    /// This is the whole point of publishing plain text, stated as the property that
    /// makes the corruption class unreachable rather than as the absence of a symptom.
    /// GTK's default rich content is re-inserted one chunk per syntax-highlight tag
    /// toggle, and a toggle inside a `\r\n` is what lets any payload-repairing handler
    /// turn it into `\n\n` (ScrAP-312). With a string on the clipboard the paste is
    /// deserialised into a fresh, UNTAGGED buffer, so there are no toggles to chunk on.
    ///
    /// **The document is chosen, not incidental.** An unclosed fenced code block leaves
    /// a `no-spell-check` context open at EOF, which is what puts a tag toggle on the
    /// final `\n`; `ensure_highlight` is what makes the tags real. Before this change
    /// this exact document pasted as TWO emissions with the first ending `0d`. If the
    /// emission count ever goes above one again, the mechanism is back whether or not
    /// anything currently exploits it.
    /// The middle-click paste must be a CAPTURE-phase controller on button 2.
    ///
    /// Both properties are load-bearing and neither is visible from the paste's result on a
    /// headless run. MEASURED in `probes/middleclick-primary-paste.m`: GTK's own click
    /// gesture is added with no explicit phase, so it runs in BUBBLE; a capture-phase gesture
    /// runs first and its claim DENIES GTK's branch, and the text arrives once. Demote this
    /// to bubble and the claim is too late — GTK pastes as well and the text arrives TWICE,
    /// which no assertion about "did a paste happen" would catch.
    ///
    /// ⚠ WHAT THIS DOES NOT COVER, stated because the test's name invites the wider reading.
    /// It asserts that `wire_middle_click_paste` INSTALLS the right gesture — it calls that
    /// function itself, through `editor()`. It therefore says NOTHING about whether the
    /// production path still calls it. MEASURED: deleting the call from
    /// `window::tabs::lifecycle` leaves this test GREEN, so a reader who takes it as "the
    /// editor has a middle-click paste" is over-crediting it by exactly one link.
    /// Nor can it drive a real middle click, which needs synthetic input and an unlocked
    /// session. The behaviour is covered by `probes/middleclick-primary-paste.m` and the call
    /// site by `tests/MANUAL-TEST.md` §1.11b.
    /// **The link the test above cannot see: does the PRODUCTION path still wire it?**
    ///
    /// Built through `new_window`, so the tab's editor is the one `build_tab_editor`
    /// assembles rather than a stand-in this module wired itself — which is the whole
    /// distinction. The macOS seat recorded the gap and judged it too expensive to close,
    /// having found no helper that reaches the editor through the production path; the
    /// swap-recovery tests already use one, so it costs a window.
    ///
    /// Asserts only what it can see structurally: a capture-phase gesture on button 2 is
    /// present on the view a real tab hands over. It says nothing about what that gesture
    /// DOES — that is the test above, and the behaviour is
    /// `probes/middleclick-primary-paste.m`.
    #[gtktest::test]
    fn the_production_editor_has_the_middle_click_gesture_wired() {
        use gtk::prelude::*;
        let app = crate::window::gtk_integration_tests::test_app(
            "com.extollit.scribobulate.it.midclickprod",
        );
        let win = crate::window::new_window(&app, "IT", "body text", None);
        let tab = crate::winstate::state(&win).expect("a tab");

        let model = tab.editor.observe_controllers();
        let mut found = false;
        for i in 0..model.n_items() {
            let Some(ctrl) = model.item(i).and_downcast::<gtk::EventController>() else {
                continue;
            };
            if ctrl.propagation_phase() != gtk::PropagationPhase::Capture {
                continue;
            }
            if let Some(click) = ctrl.downcast_ref::<gtk::GestureClick>() {
                if click.button() == gtk::gdk::BUTTON_MIDDLE {
                    found = true;
                }
            }
        }
        assert!(
            found,
            "the editor a real tab builds has no capture-phase button-2 gesture — the \
             production call site is gone, which the wiring test above cannot detect"
        );
    }

    #[gtktest::test]
    fn wiring_the_middle_click_paste_installs_a_capture_phase_button_two_gesture() {
        let (_buf, view) = editor("alpha beta");

        let click = find_middle_click_gesture(&view).expect(
            "no button-2 GestureClick on the editor view: the middle-click paste is not wired, \
             so GTK's rich buffer-to-buffer paste is what runs (ScrAP-312)",
        );
        assert_eq!(
            click.propagation_phase(),
            gtk::PropagationPhase::Capture,
            "the middle-click gesture must be in the CAPTURE phase; in bubble its claim lands \
             after GTK's own click gesture has already pasted, and the text arrives twice"
        );
    }

    /// **M46 evidence, part 1 — what the preview actually puts on PRIMARY today.**
    ///
    /// `clipboard.rs`'s module doc and `sdd/TECH.md` used to claim this module "covers
    /// both clipboards". It never took PRIMARY over on the preview pane — the publisher-side
    /// design that could have (`wire_primary_selection`, referenced above but deleted with
    /// the take-over itself) was measured to destruction because the preview swaps its
    /// buffer on every re-render, which is exactly the case that design could not survive.
    /// So an ordinary, un-taken-over `GtkTextView` selection — which is what the preview
    /// still is — publishes GTK's default rich `GtkTextBufferContent`, the same content
    /// type whose buffer-to-buffer paste chunks on syntax-highlight tag toggles
    /// (ScrAP-312). This pins that fact so it cannot silently change without this test
    /// noticing.
    #[gtktest::test]
    fn a_preview_selection_still_publishes_gtks_default_rich_content_to_primary() {
        use crate::codeview::CodePreviewView;

        let view = CodePreviewView::new();
        view.buffer().set_text("alpha beta gamma");
        let (win, ctx) = present(&view);

        view.buffer()
            .select_range(&view.buffer().start_iter(), &view.buffer().end_iter());
        pump(&ctx, 200);

        // ⚠ Assert the PROVIDER's own advertised formats, never `Clipboard::formats()`.
        // MEASURED, and it makes the obvious assertion a gate that cannot fail: the
        // clipboard's format list is a UNION with every gtype reachable through a
        // registered DESERIALIZER (GTK4Rs/AP-306; GTK4Rs/AP-285 is the same union on the drop side), and GTK registers
        // `text/plain;charset=utf-8` → `GTK_TYPE_TEXT_BUFFER` when GtkTextBuffer's class
        // initialises. So in ONE process, the identical `set_text("plain")` reports
        // `gchararray text/plain;…` before any GtkTextBuffer exists and
        // `gchararray GtkTextBuffer text/plain;…` after one does — `contains_type` there
        // answers "has a GtkTextBuffer been initialised in this binary", not "is the
        // content rich", and every test binary containing an editor answers yes.
        let provider = view
            .primary_clipboard()
            .content()
            .expect("a local selection must publish a content provider");
        let formats = provider.formats();
        assert!(
            formats.contains_type(gtk::TextBuffer::static_type()),
            "a preview selection no longer publishes GTK_TYPE_TEXT_BUFFER to PRIMARY — if a \
             take-over landed on the preview, this test (and the M46 scoping it pins) must be \
             revisited: {}",
            formats.to_str()
        );

        win.destroy();
    }

    /// **M46 evidence, part 2 — the gap part 1 proves does not reopen ScrAP-312.**
    ///
    /// A preview selection publishes GTK's default rich content (proved above). The
    /// destination editor's middle-click gesture never asks PRIMARY for
    /// `GTK_TYPE_TEXT_BUFFER` — it reads TEXT (`read_text_async`), which GDK can satisfy
    /// from ANY publisher by falling back to its buffer→text serializer. So the tag-toggle
    /// chunking ScrAP-312 names never runs on this path, whichever pane the selection came
    /// from: a same-application `insert-text` emission count of one, not per-tag-toggle.
    ///
    /// Drives the gesture's own `pressed` signal directly (GTK4Rs/AP-169) rather than a
    /// real X11 click, which Xvfb cannot deliver deterministically (GTK4Rs/AP-245); the
    /// gesture's installation and CAPTURE phase are asserted separately above; this test
    /// is only about what the handler DOES once it runs.
    #[gtktest::test]
    fn a_preview_selection_pastes_into_the_editor_as_one_plain_text_emission() {
        use std::cell::Cell;
        use std::rc::Rc;

        use crate::codeview::CodePreviewView;

        let crlf = "before\r\nafter\r\n";
        let source = CodePreviewView::new();
        source.buffer().set_text(crlf);
        let (src_win, ctx) = present(&source);
        source
            .buffer()
            .select_range(&source.buffer().start_iter(), &source.buffer().end_iter());
        pump(&ctx, 200);

        // Non-empty and not at the origin: `iter_at_location` is a glyph hit-test that
        // misses at the left margin and past a short/empty line's end (GTK4Rs/AP-15) —
        // an empty destination buffer has nowhere for a click to land at all. Derive the
        // click point from a real character's own geometry instead of guessing pixels.
        let (dest_buf, dest_view) = editor("hello world");
        let (dest_win, _) = present(&dest_view);
        pump(&ctx, 400);
        let target = dest_buf.iter_at_offset(6); // just after the space
        let rect = dest_view.iter_location(&target);
        let (wx, wy) = dest_view.buffer_to_window_coords(
            gtk::TextWindowType::Widget,
            rect.x() + 1,
            rect.y() + 1,
        );

        let runs = Rc::new(Cell::new(0usize));
        let counter = Rc::clone(&runs);
        dest_buf.connect_insert_text(move |_, _, _| counter.set(counter.get() + 1));

        let gesture = find_middle_click_gesture(&dest_view)
            .expect("wire_middle_click_paste must install the button-2 gesture");
        gesture.emit_by_name::<()>("pressed", &[&1i32, &(wx as f64), &(wy as f64)]);

        let expected = format!("hello {crlf}world");
        for _ in 0..600 {
            if dest_buf.char_count() as usize >= expected.chars().count() {
                break;
            }
            pump(&ctx, 1);
        }

        assert_eq!(
            runs.get(),
            1,
            "a middle-click paste of a PREVIEW selection must arrive as ONE `insert-text` \
             emission — more than one means the destination asked PRIMARY for a rich buffer \
             and ScrAP-312's chunking is reachable again for a cross-pane paste"
        );
        assert_eq!(
            text_of(&dest_buf),
            expected,
            "every byte of the preview's selection, CRLF included, must survive the cross-pane \
             paste unmodified"
        );

        src_win.destroy();
        dest_win.destroy();
    }

    #[gtktest::test]
    fn a_same_application_paste_arrives_as_a_single_emission() {
        use std::cell::Cell;
        use std::rc::Rc;

        let crlf = "# CRLF doc\r\n\r\n```rust\r\nfn main() {}\r\n";
        let (buf, view) = editor(crlf);
        let (win, ctx) = present(&view);
        buf.ensure_highlight(&buf.start_iter(), &buf.end_iter());

        buf.select_range(&buf.start_iter(), &buf.end_iter());
        view.emit_copy_clipboard();
        pump(&ctx, 200);

        let (dest, dest_view) = editor("");
        let (dest_win, _) = present(&dest_view);
        let runs = Rc::new(Cell::new(0usize));
        let counter = Rc::clone(&runs);
        dest.connect_insert_text(move |_, _, _| counter.set(counter.get() + 1));

        dest_view.emit_paste_clipboard();
        crate::testpump::until_for(
            crate::testpump::Clock::Idle,
            std::time::Duration::from_millis(3000),
            "the pasted text to land in the destination buffer",
            || dest.char_count() as usize >= crlf.chars().count(),
        );

        assert_eq!(
            runs.get(),
            1,
            "a same-application paste must arrive as ONE emission; more than one means \
             tagged content is back on the clipboard and the CRLF split is reachable again"
        );
        let pasted = text_of(&dest);
        assert_eq!(
            pasted.matches('\r').count(),
            crlf.matches('\r').count(),
            "every carriage return must survive the paste: got {pasted:?}"
        );
        assert_eq!(
            pasted.matches('\n').count(),
            crlf.matches('\n').count(),
            "and no line feed may have been invented in place of one"
        );

        win.destroy();
        dest_win.destroy();
    }

    /// **Lone CRs pasted from outside are repaired, and the CRLF beside them is not.**
    ///
    /// This is the route the defect was originally reported through — text arriving on
    /// the clipboard from a keyboard/mouse sharing tool with classic-Mac-OS line
    /// endings. It is repaired by `lineendings::wire_paste_normalization`, and it is only SAFE to repair here because the paste arrives as a single
    /// emission; the same repair against GTK's default tagged clipboard content is what
    /// corrupted `\r\n` (ScrAP-312). Asserting both in one test is deliberate: a repair
    /// that fixed the lone CRs and ate a CRLF would be the original bug wearing the
    /// fix's clothes.
    #[gtktest::test]
    fn a_foreign_paste_of_lone_crs_is_repaired_without_touching_crlf() {
        let (_buf, view) = editor("");
        let (win, ctx) = present(&view);

        // What a sharing tool's clipboard bridge actually delivers: a plain string.
        view.clipboard()
            .set_text("# Title\rProse.\r\r- alpha\r- beta\r\nkept\r\n");
        pump(&ctx, 100);
        view.emit_paste_clipboard();
        pump(&ctx, 400);

        let got = text_of(&view.buffer().downcast::<sourceview::Buffer>().unwrap());

        // A positive control FIRST: every assertion below is about the shape of the
        // pasted text, and all of them are satisfiable by text that never arrived.
        // Without this, a paste that silently did nothing reads as a clean pass.
        assert!(
            !got.is_empty(),
            "the paste did not land at all — nothing below is evidence about the repair"
        );

        // `has_lone_cr` is the PRODUCTION predicate, not a restatement of it. This was
        // `!got.contains('\r') || got.contains("\r\n")`, which is not the property it
        // claimed: it passes on `"a\rb\r\nc"`, a buffer holding a lone CR, because one
        // CRLF anywhere satisfies the right-hand side for the whole string.
        assert!(
            !crate::lineendings::has_lone_cr(&got),
            "no LONE carriage return may survive into the buffer: {got:?}"
        );
        assert_eq!(
            got.matches("\r\n").count(),
            2,
            "both CRLF pairs must survive verbatim — repairing one into \\n\\n is the \
             corruption this whole arc is about: {got:?}"
        );
        assert!(
            got.starts_with("# Title\nProse.\n\n- alpha\n- beta\r\n"),
            "the lone CRs must have become line feeds: {got:?}"
        );

        win.destroy();
    }

    /// **A cut removes the text, puts it on the clipboard, and is a SINGLE undo step.**
    ///
    /// Stopping `cut-clipboard`'s emission suppresses GTK's own deletion, which happens
    /// inside the path being suppressed — so the handler has to perform it. Grouping the
    /// deletion with the copy (via `UndoGroup`, not a raw `begin_user_action`/
    /// `end_user_action` pair — see `wire_plaintext_clipboard`) is what keeps it one
    /// undo step; without
    /// it the deletion becomes its own step and one Ctrl+Z leaves the document
    /// half-restored, which is the assertion at the end here.
    #[gtktest::test]
    fn a_cut_is_one_undo_step_and_leaves_the_text_on_the_clipboard() {
        let (buf, view) = editor("KEEP<CUTME>KEEP");
        let (win, ctx) = present(&view);

        let start = buf.iter_at_offset(4);
        let end = buf.iter_at_offset(11);
        buf.select_range(&start, &end);
        view.emit_cut_clipboard();
        pump(&ctx, 200);

        assert_eq!(
            text_of(&buf),
            "KEEPKEEP",
            "the selection must actually be removed"
        );

        buf.undo();
        assert_eq!(
            text_of(&buf),
            "KEEP<CUTME>KEEP",
            "ONE undo must restore the whole cut, not half of it"
        );

        win.destroy();
    }
}
