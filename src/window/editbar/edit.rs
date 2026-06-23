//! The buffer-edit primitives shared by the format commands and the Tier-2 dialog
//! insertions: splice a computed [`format::Edit`] as one undo step, read the
//! selection, and the `win.format` entry point `apply_format`.

use super::super::*;

/// Splice a computed [`format::Edit`] into the editor buffer as ONE undo step, then
/// select its post-edit range. Shared by the inline format commands and the Tier-2
/// dialog insertions (Insert Link / Image / Table). GtkTextBuffer offsets are char
/// offsets, matching the pure core.
fn apply_edit(buf: &sourceview::Buffer, edit: &format::Edit) {
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
    apply_edit(buf, &edit);
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
    // Weak-capture the editor across the idle hop (ScrAP-60 convention parity; `grab_focus`
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

    /// The undo-double-revert bug (ScrAP-108) on the surface where it was actually
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
            "one Undo after redo+format reverted the redone bold too (ScrAP-108): \
             the redo-merge barrier was not flushed before the format edit"
        );
    }
}
