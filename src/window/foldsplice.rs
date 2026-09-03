//! Toggling a disclosure by SPLICING its region, rather than re-rendering the document.
//!
//! The sibling of [`super::foldreveal`], and the split between them is the reader's
//! intent rather than the mechanism. `foldreveal` expands a CHAIN of folds in order to
//! navigate somewhere else — the reader has asked to be moved, so a full re-render plus
//! a navigation is what they want. This is the reader toggling the block in front of
//! them and expecting to stay exactly where they are.
//!
//! This file is only the resolution: which pane, which source text, which settings.
//! `preview::splice` owns the region write and `preview::splice::install` owns adopting
//! it, including the reading position — see their module docs.

use super::*;
use crate::fold::FoldKey;
use crate::preview::SpliceVerdict;

/// Splice `key`'s fold into the active tab's preview, in place.
///
/// Anything but [`SpliceVerdict::Spliced`] means the toggle was not spliced and the
/// caller must fall back to [`super::rerender_preview_in_place`] with
/// [`RenderShape::ChangedContent`]. The verdict distinguishes a refusal made before the
/// buffer was touched (the fallback merely makes the toggle visible) from a region write
/// that failed after the delete (the fallback is what REPAIRS the pane) — see
/// [`SpliceVerdict`].
///
/// The fold state must already carry the NEW value — the toggle's handler flips it, the
/// same way it does for the re-render route.
pub(crate) fn splice_disclosure_in_place(
    window: &ApplicationWindow,
    mode: ViewMode,
    key: FoldKey,
) -> SpliceVerdict {
    let Some(st) = state(window) else {
        log::debug!("window::foldsplice: refusing key {key:?} — the window has no TabState");
        return SpliceVerdict::Untouched;
    };
    // Through the one accessor that owns the tree shape (`zoom::get_preview_view`).
    // The two refusals it used to distinguish — no pane at all, versus a pane whose
    // child is not a view — are one message now, because only the first is reachable in
    // practice and the second never named a state a reader could act on differently.
    let Some(view) = super::zoom::get_preview_view(window) else {
        log::debug!(
            "window::foldsplice: refusing key {key:?} — this window has no resolvable \
             preview view (edit-only mode, or a pane whose child is not one); nothing to \
             splice, so the caller's full re-render is what makes the toggle visible"
        );
        return SpliceVerdict::Untouched;
    };

    // The mode-appropriate source, through the one accessor that owns the rule: in
    // split mode `tab.source` is stale until a mode-switch flush (D7), so the live
    // editor buffer is authoritative; in preview mode `tab.source` is (ScrAP-35). This
    // was a second spelling of that rule, which is precisely the drift the
    // one-rule-one-implementation note on `get_preview_sw` records — so it is now the
    // same CALL rather than the same words.
    let md = st.previewed_source(mode);

    // In split mode, force the editor as scroll driver before the buffer changes, so
    // the preview's own settling writes cannot drive the editor (GTK4Rs/AP-16). The
    // splice provokes far fewer of them than a re-render does — no collapse, no
    // top-down revalidation of the whole document — but "fewer" is not "none", and this
    // is the established answer to the hazard rather than a second one.
    if mode == ViewMode::Split {
        st.scroll.driver.set(ScrollDriver::Editor);
        st.scroll.pv_last.set((-1.0, -1.0));
    }

    let verdict = crate::preview::splice_disclosure(
        &view,
        crate::preview::SpliceInputs {
            fold_epoch: st.fold_epoch(),
            md: &md,
            doc_dir: st.doc_dir().as_deref(),
            zoom: st.chrome().zoom_level.get(),
            allow_unsafe_images: st.allow_unsafe_images.get(),
            folds: &st.folds.borrow(),
        },
        key,
    );

    if verdict.spliced() && mode == ViewMode::Split {
        // The same coalesced re-projection every other in-place render queues, so at
        // least one editor→preview projection fires against the new heights.
        queue_scroll_sync(window);
    }
    verdict
}
