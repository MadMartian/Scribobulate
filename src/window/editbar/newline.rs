//! Enter conveniences for the editor: auto-continue Markdown lists and blockquotes,
//! and auto-close a lone code fence — mapping the pure `format::line_continuation` /
//! `format::code_fence_close` decisions to buffer edits inside one undo step, driven
//! by a **keystroke**.
//!
//! # Why a key controller and not an `insert-text` hook (ScrAP-199 — silent paste loss)
//!
//! The obvious implementation — hook `GtkTextBuffer::insert-text` and treat an
//! inserted `"\n"` as "the user pressed Enter" — is **unsound**, and it silently ate
//! the user's text for as long as it shipped.
//!
//! A paste of same-app content is not one insertion. `gtk_text_buffer_paste_clipboard`
//! hardcodes the rich path (`gdk_clipboard_read_value_async` for `GTK_TYPE_TEXT_BUFFER`,
//! `gtktextbuffer.c:3982`) and lands in `insert_range_not_inside_self` (`:1616-1664`),
//! which walks the copied buffer with `gtk_text_iter_forward_to_tag_toggle` and emits
//! **one `insert-text` per tag-delimited run** (`:1394`). A source line that ends
//! inside a syntax-highlight tag — `` `code` ``, `**bold**` — therefore hands its
//! trailing newline over as a **bare `"\n"` run of its own**, which no content-based
//! test can tell from a keystroke: it genuinely *is* exactly `"\n"`.
//!
//! Worse than a false positive, acting on it is undefined behaviour. The `location`
//! iter is passed `G_SIGNAL_TYPE_STATIC_SCOPE` (`:581`) — **not a copy** — so it is
//! `insert_range`'s own destination cursor. The signal's contract (`:564-567`) says a
//! handler running before the default must not invalidate it; the hook invalidated it
//! by inserting, *and* suppressed the default handler that would have revalidated it.
//! Downstream, `_gtk_text_btree_insert` reads the stale `GtkTextLine*` without
//! validating it and `gtk_text_iter_get_offset` returns the invalid-iterator sentinel
//! 0 — which is why the remaining runs were observed "inserting at offset 0" and then
//! vanishing. The 26 `Invalid text buffer iterator` warnings the repro emitted are GTK
//! reporting it after the damage. On other data the same state corrupts the B-tree
//! rather than merely losing the tail.
//!
//! Guarding the hook was tried and rejected. A "was a key pressed?" token leaves the
//! same UB one keystroke-shaped path away (Ctrl+Z replaying a deleted newline is a
//! keystroke too), and an "are we pasting?" flag cannot even see the cases it must
//! exclude: middle-click PRIMARY paste calls `gtk_text_buffer_paste_clipboard`
//! directly without emitting `GtkTextView::paste-clipboard` (`gtktextview.c:5597-5607`,
//! measured here), and a failed clipboard read never closes its user action
//! (`:3767-3769`), arming such a flag for the rest of the session. The decision has to
//! be made where the keystroke is, so that paste, drag-and-drop, undo replay and every
//! programmatic `insert_range` cannot reach it **at all**.
//!
//! GtkSourceView's `GtkSourceIndenter` is that place and would be the sanctioned
//! implementation — but it is **unusable from gtk-rs at sourceview5 0.10**: the
//! subclass trampoline takes the caller's transfer-none `GtkTextIter*` with
//! `from_glib_full` and frees GtkSourceView's own iterator when the wrapper drops
//! (`subclass/indenter.rs:107-110`). Measured: a single Enter SIGSEGVs, with an
//! **empty** `indent()` body — ScrAP-200. So this does by hand what the indenter would
//! have done, from the same place: GtkSourceView installs its own key controller at
//! CAPTURE for exactly this purpose (`gtksourceview.c:1442-1443`) and this one sits
//! beside it.

use super::super::*;

/// One editor buffer mutation to run in place of a plain newline. Char offsets,
/// matching the pure core.
#[derive(Debug)]
enum NewlinePlan {
    /// Insert `text` at the caret, then place the caret at absolute offset `caret`.
    Insert { text: String, caret: i32 },
    /// Delete the first `len` chars of buffer line `line` (insert nothing).
    ClearLine { line: i32, len: i32 },
}

/// Decide what an Enter at the caret should do, reading only the buffer. `None` means
/// "a plain newline" — the caret's line is not a list item, blockquote or lone opening
/// fence, or the caret sits inside the marker run itself.
fn plan_for_caret(buf: &gtk::TextBuffer) -> Option<NewlinePlan> {
    let caret = buf.iter_at_mark(&buf.get_insert());
    let line = caret.line();
    let caret_off = caret.offset();
    let cursor_col = caret.line_offset() as usize;
    let line_start = buf.iter_at_line(line)?;
    let mut line_end = line_start;
    if !line_end.ends_line() {
        line_end.forward_to_line_end();
    }
    let line_text =
        crate::saferizer::BufferText::of_range(buf, &line_start, &line_end).into_string();

    // Code fence takes precedence (a fence line is never a list/quote line, so the two
    // never actually overlap — this just documents the intent). The document text
    // strictly above this line — needed only to tell an OPENING lone fence (auto-close
    // it) from a CLOSING one (leave it) — is built LAZILY: `code_fence_close` forces
    // it only once `line_text` is already a lone-fence candidate. Unconditionally
    // copying the WHOLE document above the caret on every Enter (U-6) cost ~30ms on a
    // 10 MB document, for a check almost every Enter never needed.
    if let Some(fence) = format::code_fence_close(
        || {
            crate::saferizer::BufferText::of_range(buf, &buf.start_iter(), &line_start)
                .into_string()
        },
        &line_text,
        cursor_col,
    ) {
        // "\n" ends the fence line, then an empty middle line, then the closing
        // delimiter. The caret lands on the empty middle line.
        return Some(NewlinePlan::Insert {
            text: format!("\n\n{}{}", fence.indent, fence.fence),
            caret: caret_off + 1,
        });
    }
    match format::line_continuation(&line_text, cursor_col)? {
        format::LineContinuation::Continue { marker } => {
            let text = format!("\n{marker}");
            // Place the caret after the freshly-inserted marker (the "insert" mark's
            // gravity would otherwise leave it before the newline).
            let caret = caret_off + text.chars().count() as i32;
            Some(NewlinePlan::Insert { text, caret })
        }
        format::LineContinuation::Clear { line_len } => Some(NewlinePlan::ClearLine {
            line,
            len: line_len as i32,
        }),
    }
}

/// Convenience edits when the user presses **Enter** in the editor: auto-continue
/// Markdown lists and blockquotes, and auto-close a lone code fence. On the caret's
/// line:
/// * a list item or blockquote with content → start the next item/quote line
///   (same bullet / `n+1`, blockquote prefix + indentation preserved); an empty
///   one → clear its marker instead of adding a line ([`format::line_continuation`]);
/// * a lone opening code fence (```` ``` ```` on its own) → drop a matching closing
///   fence on the line below and leave the caret on the empty middle line
///   ([`format::code_fence_close`]).
///
/// Everything else — every other key, every line the pure core declines, and every
/// newline the user did not type — is left to GTK untouched. See the module docs for
/// why that boundary is the whole point.
pub(crate) fn wire_newline_edits(view: &sourceview::View) {
    let keys = gtk::EventControllerKey::new();
    // CAPTURE, because GtkTextView commits the newline from its own key controller
    // (gtktextview.c:5447-5478) which is bubble-phase: a bubble-phase controller of
    // ours would only run once the newline was already in the buffer.
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(glib::clone!(
        #[weak]
        view,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |_, keyval, _, state| {
            // Enter with no modifiers. Shift/Ctrl+Enter stay a plain line break — the
            // conventional "a newline without the list bookkeeping" escape — and every
            // other key is none of our business.
            if !matches!(
                keyval,
                gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter
            ) || !state
                .intersection(gtk::accelerator_get_default_mod_mask())
                .is_empty()
                || !view.is_editable()
            {
                return glib::Propagation::Proceed;
            }
            let buf = view.buffer();
            let has_selection = buf.has_selection();
            // With no selection the decision needs nothing but the caret's line, so
            // the ordinary case goes straight back to GTK untouched.
            if !has_selection && plan_for_caret(&buf).is_none() {
                return glib::Propagation::Proceed;
            }
            // Past here we own this Enter end to end — GTK would otherwise insert a
            // second newline after ours — so the whole gesture is one undo step.
            view.reset_im_context(); // as GtkTextView does for Return, :5449
            {
                let _grp = crate::window::undo::UndoGroup::new(&buf);
                if has_selection {
                    // Enter replaces the selection, and the continuation decision
                    // belongs to the line as it stands AFTER that.
                    buf.delete_selection(true, true);
                }
                match plan_for_caret(&buf) {
                    Some(NewlinePlan::Insert { text, caret }) => {
                        let mut at = buf.iter_at_mark(&buf.get_insert());
                        buf.insert(&mut at, &text);
                        buf.place_cursor(&buf.iter_at_offset(caret));
                    }
                    Some(NewlinePlan::ClearLine { line, len }) => {
                        let mut s = buf.iter_at_line(line).unwrap_or_else(|| buf.start_iter());
                        let mut e = buf.iter_at_offset(s.offset() + len);
                        buf.delete(&mut s, &mut e);
                    }
                    // Reachable only via the selection branch: deleting it left an
                    // ordinary line. Finish the Enter here rather than propagating,
                    // or the deletion and the newline become two undo steps.
                    None => {
                        let mut at = buf.iter_at_mark(&buf.get_insert());
                        buf.insert(&mut at, "\n");
                    }
                }
            }
            // GtkTextView scrolls the caret back into view after committing text
            // (:8520); ours must too, or an Enter at the bottom of the viewport types
            // off-screen.
            view.scroll_mark_onscreen(&buf.get_insert());
            glib::Propagation::Stop
        }
    ));
    view.add_controller(keys);
}

/// GTK-object tests for the Enter conveniences (`#[gtktest::test]`, feature-gated —
/// see `src/gtk_suite.rs`).
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    fn editor(md: &str) -> (sourceview::Buffer, sourceview::View) {
        // The same pair `build_tab_editor` wires, built here so the test names the
        // unit under test directly. The markdown language is required: it is what
        // produces the highlight tags that split a paste into runs.
        let buf = sourceview::Buffer::new(None);
        buf.set_language(
            sourceview::LanguageManager::default()
                .language("markdown")
                .as_ref(),
        );
        buf.set_enable_undo(true);
        buf.set_text(md);
        let view = sourceview::View::with_buffer(&buf);
        view.set_editable(true);
        wire_newline_edits(&view);
        (buf, view)
    }

    fn pump(ctx: &glib::MainContext, budget: u32, done: impl Fn() -> bool) -> bool {
        for _ in 0..budget {
            if done() {
                return true;
            }
            if !ctx.iteration(false) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        done()
    }

    fn text_of(buf: &sourceview::Buffer) -> String {
        crate::saferizer::BufferText::of(buf).into_string()
    }

    /// ScrAP-199 — cut/copy a run of list lines, paste it back, and the last line
    /// silently vanished. The clipboard was never at fault: the paste of same-app
    /// content is a rich-text `insert_range` that emits ONE `insert-text` per
    /// tag-delimited run, so a list line ending in a highlighted span (`` `code` ``)
    /// hands its trailing newline over as a bare `"\n"` — which the old `insert-text`
    /// hook mistook for a keypress, suppressed, and replaced, invalidating
    /// `insert_range`'s own destination iter and dropping every run after it.
    ///
    /// `ensure_highlight` is the load-bearing setup line, not decoration: it is what
    /// gives the copied region the tags that split the paste into runs. Without it the
    /// region copies as ONE run and this test passes on the broken code — which is
    /// precisely why the bug read as non-deterministic in the field, GtkSourceView
    /// highlighting being lazy.
    ///
    /// Mutation-checked (ScrAP-87): reinstating the `insert-text` hook makes this FAIL
    /// with the tail of the block missing and GTK's `Invalid text buffer iterator`
    /// warning on stderr.
    #[gtktest::test]
    fn pasting_highlighted_list_lines_keeps_every_line() {
        let md =
            "# Doc\n\n- **Given** aaa\n- **When** supplies `old_session_token`\n- **Then** ccc\n";
        let (buf, view) = editor(md);
        let win = gtk::Window::new();
        win.set_child(Some(&view));
        win.present();
        let ctx = glib::MainContext::default();
        pump(&ctx, 200, || win.is_mapped());

        // The trigger: real syntax tags over the region about to be copied.
        buf.ensure_highlight(&buf.start_iter(), &buf.end_iter());

        let text = text_of(&buf);
        let block = "- **Given** aaa\n- **When** supplies `old_session_token`\n- **Then** ccc";
        let a = text.find(block).expect("the list block is in the buffer");
        let a = text[..a].chars().count() as i32;
        let b = a + block.chars().count() as i32;
        buf.select_range(&buf.iter_at_offset(a), &buf.iter_at_offset(b));
        view.emit_copy_clipboard();
        pump(&ctx, 200, || false);

        // Paste it back at the end of the document, exactly as "move this block" does.
        buf.place_cursor(&buf.end_iter());
        let before = buf.char_count();
        view.emit_paste_clipboard();
        pump(&ctx, 600, || {
            buf.char_count() >= before + block.chars().count() as i32
        });

        let after = text_of(&buf);
        assert!(
            after.ends_with(block),
            "the whole block must paste verbatim; got tail {:?}",
            &after[after.len().saturating_sub(120)..]
        );
        assert_eq!(
            after.matches("**Then** ccc").count(),
            2,
            "the pasted copy of the last line must be present as well as the original"
        );
        win.destroy();
    }

    /// The other half of the contract, and the direction this fix could silently lose:
    /// the conveniences must still fire for a real Enter. The decision core is driven
    /// directly (a synthetic key event is not constructible from gtk-rs); the
    /// controller carrying it is verified on the running app, and the two are one
    /// `matches!` apart.
    #[gtktest::test]
    fn enter_continues_a_list_item_and_an_empty_item_exits_the_list() {
        let (buf, _view) = editor("- alpha\n");
        let tb = buf.upcast_ref::<gtk::TextBuffer>().clone();

        // Caret at the end of "- alpha" → continue with a fresh "- " item.
        buf.place_cursor(&buf.iter_at_offset("- alpha".chars().count() as i32));
        match plan_for_caret(&tb) {
            Some(NewlinePlan::Insert { text, .. }) => assert_eq!(text, "\n- "),
            other => panic!("expected a Continue plan, got {other:?}"),
        }

        // Caret on an empty item → clear it rather than opening another.
        buf.set_text("- alpha\n- \n");
        buf.place_cursor(&buf.iter_at_offset("- alpha\n- ".chars().count() as i32));
        match plan_for_caret(&tb) {
            Some(NewlinePlan::ClearLine { line, len }) => assert_eq!((line, len), (1, 2)),
            other => panic!("expected a ClearLine plan, got {other:?}"),
        }

        // An ordinary paragraph line is none of our business — the Enter propagates.
        buf.set_text("plain text\n");
        buf.place_cursor(&buf.iter_at_offset(5));
        assert!(plan_for_caret(&tb).is_none());
    }

    /// U-6: `code_fence_close`'s `text_before` is now built lazily, forced only once
    /// `plan_for_caret` has already assembled a real `line_start` iter from the live
    /// buffer. This exercises that wiring end to end — a lone opening fence WITH real
    /// document content above it (auto-close fires) and a lone fence that is actually
    /// a CLOSE (auto-close is suppressed) — not just the pure function in isolation.
    #[gtktest::test]
    fn enter_on_a_lone_fence_closes_or_not_per_the_real_document_above() {
        // No trailing newline: the caret at `char_count()` lands ON the "```" line
        // itself, not a fresh blank line after it.
        let (buf, _view) = editor("prose above\n\n```");
        let tb = buf.upcast_ref::<gtk::TextBuffer>().clone();

        // No block open above → this "```" OPENS one; auto-close it.
        buf.place_cursor(&buf.iter_at_offset(buf.char_count()));
        match plan_for_caret(&tb) {
            Some(NewlinePlan::Insert { text, .. }) => assert_eq!(text, "\n\n```"),
            other => panic!("expected an opening-fence Insert plan, got {other:?}"),
        }

        // A block IS already open above (an unclosed fence + content) → this "```"
        // CLOSES it; must NOT stack another closing fence.
        buf.set_text("```\ncode\n```");
        buf.place_cursor(&buf.iter_at_offset(buf.char_count()));
        assert!(
            plan_for_caret(&tb).is_none(),
            "a closing fence must propagate as a plain newline, not auto-close again"
        );
    }
}
