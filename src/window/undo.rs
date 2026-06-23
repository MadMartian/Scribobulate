//! One typed seam for `GtkTextBuffer`'s undo grouping (the ScrAP-108
//! undo-barrier contract).
//!
//! `GtkTextBuffer`'s built-in undo (`GtkTextHistory`) sets an undo-group barrier
//! on `end_user_action` — but `redo()` and `undo()` leave the history WITHOUT
//! one. So a discrete edit made straight after a Redo merges into the redone
//! action's undo group, and a single later Undo reverts BOTH: a silent loss of
//! an edit. The fix is an empty `begin_user_action()`/`end_user_action()` pair
//! flushed *before* the real edit's own user action opens — the empty pair sets
//! the barrier, so the edit that follows is always its own discrete step
//! regardless of what preceded it. (Empirically, `place_cursor` does NOT set the
//! barrier; an empty user action does. An empty pair records no edit, so it
//! neither creates a spurious undo entry nor clears the redo stack.)
//!
//! That barrier flush is easy to forget: all three of the buffer's discrete-edit
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
//! calls inside this module carry an explicit `#[allow]`: this is the one place
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
        // Empty pair FIRST — this is the barrier flush the whole seam exists for.
        #[allow(clippy::disallowed_methods)]
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
        #[allow(clippy::disallowed_methods)]
        self.buf.end_user_action();
    }
}
