//! Making content inside a collapsed disclosure reachable: expand the folds that
//! hide it, re-render, then act on the render that results.
//!
//! Two features need exactly this and would otherwise each invent it — the outline
//! navigating to a heading inside a collapsed block (rubric 12.22) and find landing
//! on a match inside one (rubric 11.10). Both are the same three steps in the same
//! order, and both fail the same way if the third runs against the old render, so
//! the sequencing lives here once rather than twice (POLICY "prefer extending an
//! existing code path").

use super::*;
use crate::fold::FoldKey;

/// Expand every fold in `chain`, re-render the preview, and then run `after` against
/// the window — with the new render in place, so `after` may read buffer offsets
/// that only exist once the block is open.
///
/// An empty `chain` means nothing is hidden: `after` runs immediately, with no
/// re-render. That is not an optimisation but the common case — the great majority
/// of navigations are to content already on screen, and a re-render there would
/// throw away the reading position for nothing.
///
/// # Why the re-render is deferred to an idle
///
/// A re-render unparents and rebuilds every anchored child of the preview, and the
/// callers reach here from inside a widget's own signal handler — an outline row's
/// `row-activated`, a find-bar button's `clicked`. Tearing down a widget subtree
/// synchronously inside a handler that is still on the stack is the hazard
/// GTK4Rs/AP-30 records, and the disclosure toggle's own handler already defers for
/// it. `after` is queued on the SAME idle, after the re-render, rather than on a
/// second one: two idles would let anything else scheduled in between observe the
/// half-finished state, and the caller would have no way to know its follow-up had
/// been overtaken.
pub(crate) fn reveal_folds(
    window: &ApplicationWindow,
    chain: &[FoldKey],
    after: impl FnOnce(&ApplicationWindow) + 'static,
) {
    if chain.is_empty() {
        after(window);
        return;
    }
    let Some(st) = state(window) else { return };
    {
        // Every key in the chain is one this render reported as COLLAPSED, so a
        // toggle expands it. Taken as a chain rather than one key at a time because
        // a disclosure nested inside a collapsed one renders nothing — opening the
        // outer block alone would only reveal the inner block's summary line, and
        // the caller would have to re-render once per level of nesting to discover
        // that (`renderer::CollapsedSite`).
        let mut folds = st.folds.borrow_mut();
        for key in chain {
            folds.toggle(*key);
        }
    }
    let mode = st.view_mode.get();
    let win = window.downgrade();
    glib::idle_add_local_once(move || {
        let Some(window) = win.upgrade() else { return };
        crate::window::rerender_preview_in_place(&window, mode, RenderShape::ChangedContent);
        after(&window);
    });
}
