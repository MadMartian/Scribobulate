//! One typed seam for `GtkTextBuffer`'s undo grouping (GTK4Rs/AP-110 and GTK4Rs/AP-397: the
//! undo-barrier contract).
//!
//! **How `GtkTextHistory` separates steps** (gtktexthistory.c, unchanged 4.6.9–4.24.1;
//! researcher-read, and the two claims about the empty pair re-measured here): a user
//! action becomes an undo step closed by a BARRIER only when it recorded two or more
//! changes that do not merge — a delete and an insert, say. A user action that recorded
//! ONE change is instead hoisted out of its group and pushed as if typed, so it merges
//! with the step before it whenever the typing rules allow: an insert that begins where
//! the previous insert ended, with no newline and no word break, joins it. And
//! `undo()`/`redo()` leave the history without a trailing barrier, so the next edit
//! after a Redo can merge into the redone step.
//!
//! The empty `begin_user_action()`/`end_user_action()` pair the constructor runs first
//! does NOT set a barrier — an empty group is discarded — and it clears the redo stack
//! (measured; any edit that follows clears it anyway). It reopens a surviving group of
//! two or more changes, whose close then pushes a fresh barrier; it cannot help after a
//! hoisted single change. So an edit that must stay its own step records two changes:
//! `window::annotate::splice_minimal` rewrites a neighbouring character with a pure
//! insertion for exactly this reason. There is no public barrier API through 4.24.1.
//!
//! The pair is easy to forget: all three of the buffer's discrete-edit
//! routines DO wrap their edit in a `begin_user_action`/`end_user_action` pair,
//! so grepping for the obvious symbol finds nothing missing — the distinguishing
//! feature is the *empty pair flushed first*, and it lived at only one of the
//! three. Fixing it a fourth-site-at-a-time is how it kept coming back.
//!
//! [`UndoGroup`] makes the whole contract one object. Its constructor flushes the
//! barrier and opens the edit's user action; its `Drop` closes it. A discrete
//! edit is `let _grp = UndoGroup::new(&buf);` followed by the delete/insert — the
//! barrier cannot be omitted because there is no longer a separate step to omit,
//! and the closing `end_user_action` cannot be forgotten because `Drop` runs it
//! on every path out of the scope (including a `?` or an early return).
//!
//! Raw `TextBufferExt::begin_user_action` / `end_user_action` are banned via
//! `clippy.toml`'s `disallowed-methods`, so this seam is the only route to a
//! user action — a new discrete-edit routine cannot re-introduce the bug. The two
//! calls inside this module carry an explicit `#[expect]`: this is the one place
//! the raw calls are correct, by construction.

use gtk::prelude::*;

/// An open `GtkTextBuffer` user action that is guaranteed to be its own discrete
/// undo step. Construct it, perform the edit, let it drop.
///
/// Generic over any `IsA<gtk::TextBuffer>` so both the editor's
/// `sourceview::Buffer` and a plain `gtk::TextBuffer` (the annotation path) use
/// the one seam.
#[must_use = "an UndoGroup that is dropped immediately closes an empty user \
              action; bind it to a scoped `_grp` and perform the edit while it lives"]
pub(crate) struct UndoGroup {
    buf: gtk::TextBuffer,
}

impl UndoGroup {
    /// Flush the redo-merge barrier, then open the user action the caller's edit
    /// will be recorded in. The edit runs while the returned guard is alive; the
    /// guard's `Drop` closes the action.
    pub(crate) fn new(buf: &impl IsA<gtk::TextBuffer>) -> Self {
        let buf = buf.upcast_ref::<gtk::TextBuffer>().clone();
        // Empty pair FIRST: it re-closes a surviving multi-change group with a fresh
        // barrier. It is NOT a barrier of its own (module doc).
        #[expect(clippy::disallowed_methods)]
        {
            buf.begin_user_action();
            buf.end_user_action();
            // Open the real action; `Drop` closes it.
            buf.begin_user_action();
        }
        Self { buf }
    }
}

impl Drop for UndoGroup {
    fn drop(&mut self) {
        #[expect(clippy::disallowed_methods)]
        self.buf.end_user_action();
    }
}
