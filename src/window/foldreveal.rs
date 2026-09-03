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

/// Run `f` on the next main-loop idle, with the window — or not at all, if the window
/// has gone by the time the idle fires.
///
/// **The capture is WEAK, and that is the whole point of the helper.** POLICY's
/// "widget-owned closures capture weakly" rule (ScrAP-60) is easy to satisfy at the site
/// you are looking at and easy to miss at the next one: these deferrals are armed from
/// inside a widget's own signal handler, so a strong `ApplicationWindow` in the closure
/// keeps the whole window tree alive for as long as the idle is pending. Two call sites
/// had drifted apart on exactly this — `reveal_folds` downgraded and the disclosure
/// toggle's handler did not — which is the shape a shared helper removes rather than
/// documents.
///
/// **Why the deferral itself is needed** is the other half, and it is the same at both
/// sites: a re-render unparents and rebuilds every anchored child of the preview, and
/// both callers reach here from inside a widget's own signal handler (an outline row's
/// `row-activated`, a find-bar button's `clicked`, a toggle's `toggled`). Tearing down a
/// widget subtree synchronously inside a handler still on the stack is the hazard
/// GTK4Rs/AP-30 records.
pub(crate) fn defer_with_window(
    window: &ApplicationWindow,
    f: impl FnOnce(&ApplicationWindow) + 'static,
) {
    let win = window.downgrade();
    glib::idle_add_local_once(move || {
        let Some(window) = win.upgrade() else { return };
        f(&window);
    });
}

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
        // `set_collapsed(.., false)`, never `toggle`. This function's NAME is the
        // postcondition — every key in `chain` is expanded when it returns — and a
        // toggle only delivers that while every key really is collapsed, an invariant
        // held by the caller's caller and enforced by nothing. Hand it an already
        // expanded block and a toggle CLOSES one the reader asked to see, which is the
        // exact inversion of what a reveal is for.
        //
        // The document's own `<details open>` is what the fold model needs to answer
        // that, so the spans are scanned here. Taken as a chain rather than one key at
        // a time because a disclosure nested inside a collapsed one renders nothing —
        // opening the outer block alone would only reveal the inner block's summary
        // line, and the caller would have to re-render once per level of nesting to
        // discover that (`renderer::CollapsedSite`).
        //
        // **The CLEANED text, in the same coordinate space the keys are in.** This read
        // the raw source, and a `FoldKey` is an offset into the text the RENDERER
        // walked — CriticMarkup extracted. On a document with an annotation above a
        // disclosure the two disagree, the lookup below found no span, and the
        // diverged-key fallback flipped the fold instead of expanding it (F-SEC-209).
        let md = st.previewed_cleaned(st.view_mode.get());
        let spans = crate::renderer::disclosure::scan_document(&md);
        // The DECISION is `FoldState`'s and is unit-tested there; this file owns only
        // the wiring around it and the diagnostic for what it reports back.
        let diverged = st.folds.borrow_mut().expand_chain(&spans, chain);
        for key in diverged {
            log::error!(
                "window::foldreveal: key {key:?} names no disclosure in the current \
                 source; flipping it rather than expanding it"
            );
        }
    }
    let mode = st.view_mode.get();
    defer_with_window(window, move |window| {
        crate::window::rerender_preview_in_place(window, mode, RenderShape::ChangedContent);
        after(window);
    });
}
