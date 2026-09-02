//! The fold splice: changing one disclosure's rendered region in place, instead of
//! rebuilding the whole buffer.
//!
//! **Why this module exists.** Re-rendering a document in place discards every line's
//! height validation, which collapses the vadjustment's `upper` and clamps `value` to
//! zero; GTK then re-validates top-down over many idle passes before the reading
//! position can be restored. The reader sees the document drop to its top and glide
//! back. The landing position is exact — it is the journey that is visible — and the
//! same trajectory follows every in-place re-render, a zoom step included. A targeted
//! edit does not pay that cost: an insert mid-document on a validated view moves
//! `upper` and `value` by the edit's own height, with no collapse.
//!
//! **The shape, and the one claim it rests on.** A toggle re-walks the document into a
//! scratch buffer to rebuild every offset map, and splices only the toggled block's
//! region into the live buffer. That is correct precisely because a fold changes
//! exactly one contiguous region of the rendered text: everything before the block
//! renders identically under either fold state, and everything after it is identical
//! but shifted. [`tests`] proves that rather than assuming it — the assumption is the
//! route's only real risk, and a one-character divergence at a region boundary would
//! silently offset every map below the splice while every individual map still looked
//! well-formed.
//!
//! # The splice itself: [`splice`]
//!
//! Three moves, run in this order:
//!
//! 1. **PASS A** — a full walk into a SCRATCH buffer under the NEW fold state
//!    ([`super::build::build_render_products_with_theme`], unmodified). Its buffer is
//!    thrown away; every one of its maps is not — the scratch and live coordinate
//!    spaces agree everywhere once the splice below has run, so `PASS A`'s offsets are
//!    valid against the live buffer wholesale, with no translation.
//! 2. **The live delete** — [`crate::renderer::DisclosureExtent::volatile`] names the
//!    region a toggle changes; the caller's PRE-splice extent for `key` is deleted
//!    from the LIVE buffer, through the newline run that follows it (see the module
//!    docs above — `block_sep` is lazy, so that run belongs to whatever comes next,
//!    not to this block).
//! 3. **PASS SEED+LIVE** ([`render_region`]) — a second renderer, writing at the
//!    deletion point in the LIVE buffer, seeded from a throwaway walk's state at the
//!    exact moment it reaches `key`'s region (`crate::renderer::RegionSeed`). No
//!    offset in the seed needs translating, for the same reason PASS A's maps do not:
//!    the two coordinate spaces coincide up to the region.
//!
//! The anchored children this produces are then merged with whatever survived the
//! delete (`TextChildAnchor::is_deleted()` — GTK's own delete unparents anything
//! inside the deleted range and touches nothing outside it, so this is exact, not a
//! guess), sorted by live position so a caller that pairs them positionally against
//! another PASS-A list (table cells against `cell_src_spans`) sees document order.
//!
//! # Wiring: [`install`]
//!
//! [`splice`] is the mechanism; [`install::splice_disclosure`] is the feature. It
//! captures the reader's place, runs the splice, installs the merged products into the
//! live view and its `RenderData`, and puts the reader back once GTK has stopped
//! moving the viewport. `preview::render`'s disclosure toggle calls it and falls back
//! to a full re-render whenever it answers `false`.

/// The half that turns [`splice`]'s outcome into what a reader sees: installing the
/// merged products, re-wiring the region's own controls, and holding the reading
/// position across the transition. Its own file because this one owns *producing* a
/// correct region write and that one owns *adopting* it, and because the two together
/// are past the 500-line soft limit.
pub(super) mod install;

use super::build::{build_render_products_with_theme, RenderProducts};
use crate::fold::{FoldKey, FoldState};
use crate::renderer::{md_options, DisclosureToggle, NormalizedMd, Renderer};
use gtk::prelude::*;
use gtk::TextBuffer;
use pulldown_cmark::Parser;
use std::rc::Rc;

/// Everything a fold toggle produced, ready for a caller to install: PASS A's
/// wholesale maps (`products`, `.buf`/`.anchored`/… are the throwaway scratch
/// buffer's own — a caller wants `merged_anchored`/`region`, not those), the merged
/// anchor list (survivors + the region's fresh ones, in document order), and the
/// region's own widget outputs on their own (a caller that must NOT re-parent an
/// already-live survivor — see [`super::build::attach_anchored`] — hands it only
/// [`Self::region`], never [`Self::merged_anchored`]).
pub(super) struct SpliceOutcome {
    /// PASS A's full output. Every field here except the buffer/widget ones
    /// (`buf`, `anchored`, `image_tints`, `install.width_bounded/image_bounded/tables`,
    /// `disclosure_toggles`) is valid against the LIVE buffer wholesale — which is
    /// what [`install::splice_disclosure`] installs, field for field, the way
    /// `re_render` installs a full render's.
    pub(super) products: RenderProducts,
    /// Every anchor now live in the buffer this splice was run against, in document
    /// order: the ones that survived the delete, plus [`Self::region`]'s.
    pub(super) merged_anchored: Vec<(gtk::TextChildAnchor, gtk::Widget)>,
    /// The region render's own outputs — ONLY the widgets a region render actually
    /// created, never a survivor. This is what a caller must hand to
    /// [`super::build::attach_anchored`] (which unconditionally parents every entry
    /// it is given) and what it must fold `DisclosureToggle`s from into the live
    /// `RenderData.disclosure_lines` alongside the surviving toggles.
    pub(super) region: RegionWidgets,
}

/// The widget-bearing outputs of one region render ([`render_region`]) — a subset of
/// what a full [`Renderer`] produces, because a region render's non-widget maps
/// (offsets, spans) are never read: [`SpliceOutcome::products`] already carries the
/// whole document's, from PASS A, and those are what a caller installs.
#[derive(Default)]
pub(super) struct RegionWidgets {
    pub(super) anchored: Vec<(gtk::TextChildAnchor, gtk::Widget)>,
    pub(super) image_tints: Vec<(gtk::TextChildAnchor, gtk::Widget)>,
    pub(super) width_bounded: Vec<(gtk::Widget, i32)>,
    pub(super) image_bounded: Vec<(gtk::Widget, i32, i32)>,
    pub(super) tables: Vec<crate::widgets::table::ScribTableWidget>,
    pub(super) disclosure_toggles: Vec<DisclosureToggle>,
}

/// Splice `key`'s fold toggle into the live `buf`, in place.
///
/// `old_anchored` and `old_extents` are the PRE-splice render's own — the live
/// widgets and the region `key` currently occupies, both read BEFORE this function
/// touches the buffer. `folds` already reflects the NEW state (the caller toggles it
/// before calling this).
///
/// `None` when `key` has no recorded extent in `old_extents` (nothing was drawn for
/// it last render — an ancestor was collapsed, or it was never emitted) or when PASS
/// A's own walk somehow does not draw it either (logged; should not happen while
/// both walks parse the same string with the same options, mirroring
/// `Renderer::opening_details_is_closed`'s own defensive posture). Either way the
/// caller's answer is the same: fall back to a full re-render.
#[allow(clippy::too_many_arguments)] // every parameter is a distinct render input;
                                     // see `build_render_products_with_theme`'s own
                                     // identical allowance for the same reason
pub(super) fn splice(
    buf: &TextBuffer,
    view: Option<&crate::codeview::CodePreviewView>,
    old_anchored: &[(gtk::TextChildAnchor, gtk::Widget)],
    old_extents: &[crate::renderer::DisclosureExtent],
    md: &str,
    doc_dir: Option<&std::path::Path>,
    zoom: f64,
    allow_unsafe: bool,
    theme: Rc<crate::theme::Theme>,
    folds: &FoldState,
    key: FoldKey,
) -> Option<SpliceOutcome> {
    let old_extent = old_extents.iter().find(|e| e.key == key)?;
    let old_volatile = old_extent.volatile;

    // PASS A — see module docs.
    let products =
        build_render_products_with_theme(md, doc_dir, zoom, allow_unsafe, theme.clone(), folds);
    let new_extent = products.disclosure_extents.iter().find(|e| e.key == key)?;
    let target_collapsed = new_extent.body.is_empty();

    // The live delete, through the trailing newline run.
    let mut del_start = buf.iter_at_offset(old_volatile.start);
    let mut del_end = buf.iter_at_offset(old_volatile.end);
    while !del_end.is_end() && del_end.char() == '\n' {
        del_end.forward_char();
    }
    buf.delete(&mut del_start, &mut del_end);

    // PASS SEED+LIVE.
    let region = render_region(
        buf,
        view,
        md,
        doc_dir,
        zoom,
        allow_unsafe,
        theme,
        folds,
        key,
        old_volatile.start,
        target_collapsed,
    );

    // Merge: survivors (GTK's own delete already unparented anything strictly
    // inside the deleted range and touched nothing outside it, so this is exact)
    // plus the region's fresh anchors, ordered by live position.
    let mut merged: Vec<(gtk::TextChildAnchor, gtk::Widget)> = old_anchored
        .iter()
        .filter(|(anchor, _)| !anchor.is_deleted())
        .cloned()
        .collect();
    merged.extend(region.anchored.iter().cloned());
    merged.sort_by_key(|(anchor, _)| buf.iter_at_child_anchor(anchor).offset());

    // Build-time drift guard (debug only), the same shape and the same near-total
    // oracle `build_products` runs on every full render (`preview::build`): assert
    // PASS A's copymap — a map built for the SCRATCH buffer — still matches the
    // buffer this splice just mutated. A one-character error anywhere in the
    // splice (a wrong deletion boundary, a mis-seeded region render) leaves every
    // individual map internally well-formed and only THIS check catches it.
    #[cfg(debug_assertions)]
    {
        let slice = buf.slice(&buf.start_iter(), &buf.end_iter(), true);
        let chars: Vec<char> = slice.chars().collect();
        crate::copymap::debug_verify(&products.copymap, &products.md_owned, &chars);
    }

    Some(SpliceOutcome {
        products,
        merged_anchored: merged,
        region,
    })
}

/// PASS SEED+LIVE: capture the inter-block state a full render under `folds` would
/// have at `key`'s region, then write that region into `buf` at `at_offset`.
///
/// Reparses `md` from the top rather than reusing PASS A's own walk, which costs a
/// second walk up to the region — accepted CPU, not a correctness risk (module
/// docs: "Its cost is CPU, not correctness"). The seed-capturing renderer writes
/// into its OWN throwaway scratch buffer and is discarded once it has yielded a
/// [`crate::renderer::RegionSeed`]; only the LIVE renderer this constructs from that
/// seed ever touches `buf`.
#[allow(clippy::too_many_arguments)] // every parameter is a render input `splice`
                                     // already had; this is its own walk, not a
                                     // second seam over the same inputs
fn render_region(
    buf: &TextBuffer,
    view: Option<&crate::codeview::CodePreviewView>,
    md: &str,
    doc_dir: Option<&std::path::Path>,
    zoom: f64,
    allow_unsafe: bool,
    theme: Rc<crate::theme::Theme>,
    folds: &FoldState,
    key: FoldKey,
    at_offset: i32,
    target_collapsed: bool,
) -> RegionWidgets {
    let md_norm = NormalizedMd::new(md);
    let extraction = crate::annotate::extract(md_norm.as_str());
    let cleaned = extraction.cleaned.as_str();
    let ann_highlights: Vec<(usize, usize)> = extraction
        .annotations
        .iter()
        .filter(|a| a.kind == crate::annotate::AnnKind::Highlight)
        .map(|a| (a.cleaned_content.start.raw(), a.cleaned_content.end.raw()))
        .collect();
    let palette = crate::palette::Palette::for_theme(&theme);

    let new_renderer = |buf: TextBuffer| {
        Renderer::new(
            buf,
            theme.clone(),
            palette.syntect_theme.clone(),
            doc_dir.map(|d| d.to_path_buf()),
            allow_unsafe,
            cleaned.to_string(),
            ann_highlights.clone(),
            zoom,
            folds.clone(),
        )
    };

    // The seed-capture walk (see this function's doc comment). Its buffer gets the
    // same tag table any fresh render would (`build_render_products_with_theme`'s
    // own "fresh tag table" branch) — a body event applies tags as it writes, and
    // an untagged buffer answers every `apply_tag_by_name` with a `Gtk-WARNING`
    // rather than a state difference, but there is no reason to pay the noise.
    let seed_buf = TextBuffer::new(None::<&gtk::TextTagTable>);
    crate::tags::setup_tags_with_theme(&seed_buf, &palette, zoom, &theme);
    let mut seed_r = new_renderer(seed_buf);

    let mut live: Option<Renderer> = None;
    for (ev, src) in Parser::new_ext(cleaned, md_options()).into_offset_iter() {
        match &mut live {
            None => {
                // `event_src` must be set before EVERY `process` call — a
                // disclosure's own identity (`DisclosureFrame::key`, minted from
                // `event_src.start` when its opening `<details>` is scanned) is
                // read from it, and `build_products`' own loop sets it the same
                // way for the same reason.
                seed_r.event_src = src.clone();
                seed_r.process(ev);
                if let Some(seed) = seed_r.capture_region_seed(key) {
                    let mut r = new_renderer(buf.clone());
                    // BEFORE `write_at`, so every anchored child this region render
                    // creates is parented in the same turn as its anchor — see
                    // `Renderer::push_anchored`. `None` (a bare-buffer caller, as
                    // `super::tests` is) keeps the old two-step behaviour.
                    if let Some(view) = view {
                        r.set_live_view(view);
                    }
                    r.seed(seed);
                    r.write_at(at_offset);
                    // The summary line's own tail (the collapsed preview, if any,
                    // then the newline `emit_pending_summary` always ends with) —
                    // written into the SCRATCH buffer during seed capture, never
                    // into this (live) one. See `write_seeded_summary_tail`'s doc
                    // comment for why this is a catch-up write and not double
                    // bookkeeping.
                    r.write_seeded_summary_tail(target_collapsed);
                    if target_collapsed {
                        // A collapsed target has no body events to process at all
                        // — `Renderer::process`'s own suppression would swallow
                        // them if it did.
                        r.finish_region();
                        live = Some(r);
                        break;
                    }
                    live = Some(r);
                }
            }
            Some(r) => {
                r.event_src = src;
                r.process(ev);
                if !r.is_open(key) {
                    // The event just processed closed `key`'s own frame — the
                    // region ends here. Everything after this belongs to content
                    // already sitting in the live buffer, untouched; a region
                    // render must never reach past its own block.
                    r.finish_region();
                    break;
                }
            }
        }
    }
    let mut r = live.unwrap_or_else(|| {
        log::error!(
            "preview::splice: PASS A drew a disclosure_extent for source byte {} but the \
             region-seed walk never reached it — the two walks parsed the same string with \
             the same options, so this should be unreachable; writing nothing for this \
             region",
            key.0
        );
        new_renderer(buf.clone())
    });
    // `attach_anchored`'s own trailing `queue_resize`, which the eager path would
    // otherwise skip: parenting N children queues N resizes on the view, but the
    // batched one is what the two-step route does and the arms must differ only in
    // WHEN the children are parented.
    if let Some(view) = view {
        if !r.anchored.is_empty() {
            view.queue_resize();
        }
    }
    RegionWidgets {
        anchored: std::mem::take(&mut r.anchored),
        image_tints: std::mem::take(&mut r.image_tints),
        width_bounded: std::mem::take(&mut r.width_bounded),
        image_bounded: std::mem::take(&mut r.image_bounded),
        tables: std::mem::take(&mut r.tables),
        disclosure_toggles: std::mem::take(&mut r.disclosure_toggles),
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod tests;

/// The MEASUREMENT of what this module buys — a presented pane, a reader parked well
/// down a tall document, and the vadjustment sampled across a toggle by both routes.
/// Separate from [`tests`] because it is a different shape (a live window and the
/// excursion's trough, not buffer text) and because that file was already at the
/// 500-line soft limit.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod excursion;
