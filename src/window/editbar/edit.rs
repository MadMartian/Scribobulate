//! The buffer-edit primitives shared by the format commands and the Tier-2 dialog
//! insertions: splice a computed [`format::Edit`] as one undo step, read the
//! selection, and the `win.format` entry point `apply_format`.

use super::super::*;

/// Splice a computed [`format::Edit`] into the editor buffer as ONE undo step, then
/// select its post-edit range. Shared by the inline format commands and the Tier-2
/// dialog insertions (Insert Link / Image / Table). GtkTextBuffer offsets are char
/// offsets, matching the pure core.
pub(super) fn apply_edit(buf: &sourceview::Buffer, edit: &format::Edit) {
    {
        // One discrete undo step with the redo-merge barrier flushed first
        // (`window::undo::UndoGroup`) — scoped so it closes before the
        // `select_range` below, which is not part of the edit.
        let _grp = crate::window::undo::UndoGroup::new(buf);
        let mut s = buf.iter_at_offset(edit.start as i32);
        let mut e = buf.iter_at_offset(edit.end as i32);
        buf.delete(&mut s, &mut e);
        let mut ins = buf.iter_at_offset(edit.start as i32);
        buf.insert(&mut ins, &edit.replacement);
    }

    let a = buf.iter_at_offset(edit.sel_start as i32);
    let b = buf.iter_at_offset(edit.sel_end as i32);
    buf.select_range(&a, &b);
}

/// The editor's selection as `(start, end, text)` char offsets + the selected text,
/// or the caret offset (empty text) when nothing is selected.
pub(super) fn selection_range_and_text(buf: &sourceview::Buffer) -> (usize, usize, String) {
    match buf.selection_bounds() {
        Some((s, e)) => (
            s.offset() as usize,
            e.offset() as usize,
            crate::saferizer::BufferText::of_range(buf, &s, &e).into_string(),
        ),
        None => {
            let off = buf.property::<i32>("cursor-position") as usize;
            (off, off, String::new())
        }
    }
}

pub(crate) fn apply_format(st: &crate::winstate::TabState, cmd: FormatCmd) {
    let buf = &st.editor_buf;
    let (sel_start, sel_end, _) = selection_range_and_text(buf);
    let text = st.editor_text();
    let edit = format::apply(cmd, &text, sel_start, sel_end);
    // Indent/Dedent off a list item changes nothing; an empty user action would
    // still cut an undo step and mark the tab modified.
    if !edit.is_noop() {
        apply_edit(buf, &edit);
    }
}

/// Replace the editor's `[start, end)` with `markup` as one undo step (caret at the
/// end of the inserted text), then defer focus back to the editor. Shared splice for
/// the Tier-2 dialog insertions.
pub(super) fn splice_markup(window: &ApplicationWindow, start: usize, end: usize, markup: &str) {
    let Some(st) = state(window) else { return };
    let caret = start + markup.chars().count();
    let edit = format::Edit {
        start,
        end,
        replacement: markup.to_string(),
        sel_start: caret,
        sel_end: caret,
    };
    apply_edit(&st.editor_buf, &edit);
    // Weak-capture the editor across the idle hop (GTK4Rs/AP-63 convention parity; `grab_focus`
    // is surface-independent so this is not a crash class, but a deferred idle should not
    // strong-hold a widget).
    let editor = &st.editor;
    gtk::glib::idle_add_local_once(gtk::glib::clone!(
        #[weak]
        editor,
        move || {
            editor.grab_focus();
        }
    ));
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    fn undo_enabled_source_buf(text: &str) -> sourceview::Buffer {
        let b = sourceview::Buffer::new(None);
        b.set_enable_undo(true);
        // Load the baseline as an irreversible action, exactly as the real editor
        // does at tab build — so only the format edits below sit on the undo stack.
        b.begin_irreversible_action();
        b.set_text(text);
        b.end_irreversible_action();
        b
    }

    fn text_of(b: &sourceview::Buffer) -> String {
        crate::saferizer::BufferText::of(b).into_string()
    }

    /// The undo-double-revert bug (GTK4Rs/AP-110) on the surface where it was actually
    /// live: a FORMAT command, not an annotation. Bold a word, undo, redo,
    /// italicise a different word, then a single Undo must revert ONLY the italic
    /// — the redone bold must survive.
    ///
    /// Before `UndoGroup`, `apply_edit` opened a user action but did not flush the
    /// redo-merge barrier first, so the italic merged into the redone bold's undo
    /// group and one Undo reverted both. The annotation path was guarded; this one
    /// was not, and the existing regression test pinned only the annotation path —
    /// which is exactly why this survived.
    #[gtktest::test]
    fn redo_then_format_edit_are_two_independent_undo_steps() {
        let b = undo_enabled_source_buf("alpha beta");
        let base = text_of(&b);

        // 1. Bold "alpha" (chars 0..5).
        apply_edit(&b, &format::apply(FormatCmd::Bold, &base, 0, 5));
        let after_bold = text_of(&b);
        assert_ne!(after_bold, base, "precondition: bold changed the text");

        // 2. undo, 3. redo.
        b.undo();
        assert_eq!(text_of(&b), base, "precondition: undo reverted the bold");
        b.redo();
        assert_eq!(
            text_of(&b),
            after_bold,
            "precondition: redo restored the bold"
        );

        // 4. Italicise "beta" — its offset shifted by the bold markup around alpha.
        let beta_start = after_bold
            .find("beta")
            .expect("beta survives bolding alpha");
        let beta_end = beta_start + "beta".chars().count();
        apply_edit(
            &b,
            &format::apply(FormatCmd::Italic, &after_bold, beta_start, beta_end),
        );
        let after_italic = text_of(&b);
        assert_ne!(
            after_italic, after_bold,
            "precondition: italic changed the text"
        );

        // 5. One Undo must land back at `after_bold` — the italic reverted, the
        //    redone bold intact. Landing at `base` is the OOO bug: both reverted.
        b.undo();
        assert_eq!(
            text_of(&b),
            after_bold,
            "one Undo after redo+format reverted the redone bold too (GTK4Rs/AP-110): \
             the redo-merge barrier was not flushed before the format edit"
        );
    }
}

/// TDD 10.5: a block command on a CRLF document gives exactly the result it gives on
/// the LF twin, with every line ending CRLF — through the REAL editor buffer
/// (`lineendings::new_editor_buffer`, whose lone-CR repair hook sees every spliced
/// replacement), because the pure core alone cannot see what that hook does.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod line_ending_tests {
    use super::*;

    /// Every block command — the in-place rewrites, the line-adding ones and the two
    /// list-structure edits.
    const BLOCK_CMDS: [FormatCmd; 10] = [
        FormatCmd::Heading(1),
        FormatCmd::Heading(3),
        FormatCmd::Quote,
        FormatCmd::BulletedList,
        FormatCmd::NumberedList,
        FormatCmd::TaskList,
        FormatCmd::CodeBlock,
        FormatCmd::HorizontalRule,
        FormatCmd::Indent,
        FormatCmd::Dedent,
    ];

    /// LF documents, each probed with the caret on line 2 and with a selection over
    /// lines 2–3. Includes a nested list (so Indent / Dedent and a list conversion
    /// carry a subtree) and a document already formatted (so each command also
    /// toggles off).
    const LF_DOCS: [&str; 4] = [
        "first line\nsecond line here\nthird\nfourth\n",
        "- a\n- b\n  - c\n- d\n",
        "# a\n# b\n> c\nd",
        "```\ncode\n```\n",
    ];

    /// An armed editor buffer holding `text` as a non-undoable baseline.
    fn armed_buf(text: &str) -> sourceview::Buffer {
        let b = crate::lineendings::new_editor_buffer();
        b.set_enable_undo(true);
        b.begin_irreversible_action();
        b.set_text(text);
        b.end_irreversible_action();
        b
    }

    fn text_of(b: &sourceview::Buffer) -> String {
        crate::saferizer::BufferText::of(b).into_string()
    }

    /// Apply `cmd` over the selection of `line` (0-based), `extend` lines further.
    fn run(doc: &str, cmd: FormatCmd, line: usize, extend: usize) -> String {
        // One character into line `l`, or its start when it is empty — never between
        // a CRLF's two halves, where no caret can sit.
        let offset = |l: usize| -> usize {
            let mut rows = doc.split_inclusive('\n');
            let before: usize = rows.by_ref().take(l).map(|r| r.chars().count()).sum();
            let row = rows.next().unwrap_or("");
            before + usize::from(!row.trim_end_matches(['\r', '\n']).is_empty())
        };
        let buf = armed_buf(doc);
        let edit = format::apply(cmd, doc, offset(line), offset(line + extend));
        apply_edit(&buf, &edit);
        text_of(&buf)
    }

    #[gtktest::test]
    fn every_block_command_on_a_crlf_document_matches_its_lf_twin() {
        for lf in LF_DOCS {
            let crlf = lf.replace('\n', "\r\n");
            for cmd in BLOCK_CMDS {
                for extend in [0, 1] {
                    let want = run(lf, cmd, 1, extend).replace('\n', "\r\n");
                    let got = run(&crlf, cmd, 1, extend);
                    assert_eq!(got, want, "{cmd:?} (+{extend} lines) on {crlf:?}");
                }
            }
        }
    }

    /// The repro from manual test 10.4, byte for byte — and Undo / Redo, which replay
    /// through the same insert hook, restore each side exactly.
    #[gtktest::test]
    fn heading_on_a_crlf_line_keeps_its_crlf_and_adds_no_blank_line() {
        let doc = "first line\r\nsecond line here\r\nthird\r\nfourth\r\n";
        let want = "first line\r\n# second line here\r\nthird\r\nfourth\r\n";
        assert_eq!(run(doc, FormatCmd::Heading(1), 1, 0), want);

        let buf = armed_buf(doc);
        apply_edit(&buf, &format::apply(FormatCmd::Heading(1), doc, 12, 12));
        buf.undo();
        assert_eq!(text_of(&buf), doc, "undo");
        buf.redo();
        assert_eq!(text_of(&buf), want, "redo");
    }

    /// A mixed document keeps every line's own ending under each in-place command: the
    /// CRLF line stays CRLF and the LF lines stay LF.
    #[gtktest::test]
    fn a_block_command_on_a_mixed_document_keeps_each_lines_ending() {
        let doc = "one\ntwo\r\nthree\n";
        for cmd in &BLOCK_CMDS[..6] {
            let out = run(doc, *cmd, 1, 0);
            assert_eq!(
                out.matches("\r\n").count(),
                1,
                "{cmd:?} on {doc:?} must keep line 2's CRLF and add no other: {out:?}"
            );
        }
    }
}
