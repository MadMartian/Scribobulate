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

    // In split mode the reader is in the PREVIEW — the toggle is there — so the
    // preview's own restore (`preview::splice::install`'s reader anchor) is what keeps
    // their place, and the sync is held off until it has landed. Forcing the editor as
    // driver instead (the re-render route's answer to GTK4Rs/AP-16) re-projected the
    // editor's line onto the preview on every settling write: a reader inside a
    // collapsing block was thrown to the editor's coarser line rather than the
    // summary (TDD 2.26h), and the restore and the projection then took turns. The
    // hold keeps the preview's settling writes from driving the editor as well, which
    // is the hazard the forced driver existed for. On release the editor follows the
    // preview once (`release_split_sync`).
    let on_settled = if mode == ViewMode::Split {
        st.scroll.driver.set(ScrollDriver::Preview);
        let generation = super::scrollsync::hold_split_sync(&st);
        let tab = st.id;
        crate::preview::OnSettled::new(move || {
            super::scrollsync::release_split_sync(tab, generation);
        })
    } else {
        crate::preview::OnSettled::none()
    };

    let verdict = crate::preview::splice_disclosure(
        &view,
        crate::preview::SpliceInputs {
            notice_tab: Some(st.id),
            md: &md,
            doc_dir: st.doc_dir().as_deref(),
            zoom: st.chrome().zoom_level.get(),
            allow_unsafe_images: st.allow_unsafe_images.get(),
            folds: &st.folds.borrow(),
            on_settled,
        },
        key,
    );
    verdict
}
