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
mod regionwriter;

use super::build::{Prepared, RenderProducts};
use crate::fold::FoldKey;
use crate::renderer::{md_options, DisclosureToggle};
use gtk::prelude::*;
use gtk::TextBuffer;
use pulldown_cmark::Parser;
use regionwriter::{RegionWriter, Writing};

/// Everything a fold toggle produced, ready for a caller to install: PASS A's
/// wholesale maps (`products`, `.buf`/`.anchored`/… are the throwaway scratch
/// buffer's own — a caller wants `merged_anchored`/`region`, not those), the merged
/// anchor list (survivors + the region's fresh ones, in document order), and the
/// region's own widget outputs on their own (a caller that must NOT re-parent an
/// already-live survivor — see [`super::build::attach_anchored`] — hands it only
/// [`Self::region`], never [`Self::merged_anchored`]).
pub(super) struct SpliceOutcome {
    /// PASS A's output, already reduced to the half that is valid against the LIVE
    /// buffer — see [`ScratchProducts`]. Installed wholesale, the way `re_render`
    /// installs a full render's.
    pub(super) products: ScratchProducts,
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

/// PASS A's output, reduced to the half that is valid against a DIFFERENT buffer.
///
/// A whole-document [`RenderProducts`] measures its maps against the buffer it filled
/// and OWNS every widget it created for it. PASS A's buffer is a scratch one that is
/// dropped the moment this value is built: the maps and the buffer-space decor
/// transfer to the live pane (the splice proves the two buffers hold the same
/// characters before installing them), and the widgets do not — they are children of
/// nothing, and installing one would put an unparented widget into a live view's
/// record.
///
/// **Why a separate type and not a comment.** The install used to destructure
/// `RenderProducts` and drop six fields by naming each `_`, so the only thing
/// separating the two halves was a reviewer noticing a missing underscore — and
/// adding a field produced a compile error whose obvious fix was another `_`. That is
/// the ScrAP-131 shape. The conversion below is the ONE place the split is decided,
/// and it is an exhaustive destructure, so a field added to `RenderProducts` must be
/// assigned a side there rather than silently inheriting one.
pub(super) struct ScratchProducts {
    /// Every buffer-keyed map, valid against any buffer whose text matches.
    pub(super) maps: crate::preview::build::RenderMaps,
    /// The buffer-space decorations, likewise. The widget half of
    /// [`crate::preview::build::ViewInstall`] is deliberately absent.
    pub(super) decor: crate::preview::build::InstallDecor,
    pub(super) markers: Vec<crate::codeview::MarkerData>,
    pub(super) cell_src_spans: Vec<std::ops::Range<usize>>,
}

impl From<RenderProducts> for ScratchProducts {
    fn from(products: RenderProducts) -> Self {
        let RenderProducts {
            // ── the scratch buffer's own, and everything parented in it ──
            buf: _,
            // PASS A's toggles belong to the scratch buffer; the region render's are
            // the live ones.
            disclosure_toggles: _,
            anchored: _,
            image_tints: _,
            // The tags are already on the live buffer — this render did not write it.
            highlight_ranges: _,
            // ── valid against any buffer whose text matches ──
            maps,
            install,
            markers,
            cell_src_spans,
        } = products;
        Self {
            maps,
            decor: install.decor,
            markers,
            cell_src_spans,
        }
    }
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

/// Where the region delete ends: `from`, advanced through the run of newlines that
/// follows it, stopping at the end of the buffer.
///
/// **A display-free seam over the splice's only arithmetic.** `splice` is otherwise all
/// live `GtkTextBuffer`, so this rule could previously be exercised only by rendering a
/// document and reading the text back — which tests the boundary through everything else
/// at once and cannot reach its edges (a region at the very end of the buffer, a run of
/// several blank lines, a region with no trailing newline at all). `char_at` answers
/// `None` past the end, which is what `TextIter::is_end` means at the call site.
///
/// The trailing run is taken because a block's rendered region owns the separator after
/// it: leaving it behind accumulates a blank line per toggle, and taking one too many
/// eats the next block's own.
fn delete_end_through_newlines(from: i32, char_at: impl Fn(i32) -> Option<char>) -> i32 {
    let mut end = from;
    while char_at(end) == Some('\n') {
        end += 1;
    }
    end
}

/// Why a [`splice`] produced no outcome — and, critically, whether the live buffer
/// was already mutated when it gave up.
///
/// The distinction is the whole point of this type. A caller's remedy for both is a
/// full re-render, but only one of them is OPTIONAL: after [`Self::Untouched`] the
/// pane still holds exactly what it held before, and after [`Self::RegionLost`] it is
/// missing the toggled block's whole rendered region while every map the view holds
/// still describes the longer buffer. A `bool` cannot say that, and this module's
/// contract used to assert the opposite in prose while the code returned success.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SpliceRefusal {
    /// Refused BEFORE the buffer was touched. The pane is untouched and internally
    /// consistent; a fallback re-render is the caller's choice, not an obligation.
    Untouched,
    /// The live delete ran and the region write did not, so the buffer is now short
    /// by the region's length while the installed maps are not. The caller MUST
    /// re-render the whole pane; leaving this state on screen desynchronises every
    /// offset below the splice (copy, find, outline, annotation placement).
    RegionLost,
}

/// Splice `key`'s fold toggle into the live `buf`, in place.
///
/// `old_anchored` and `old_extents` are the PRE-splice render's own — the live
/// widgets and the region `key` currently occupies, both read BEFORE this function
/// touches the buffer. `folds` already reflects the NEW state (the caller toggles it
/// before calling this).
///
/// [`SpliceRefusal::Untouched`] when `key` has no recorded extent in `old_extents`
/// (nothing was drawn for it last render — an ancestor was collapsed, or it was never
/// emitted), when its body is unspliceable literal text, when the recorded extent is
/// not a range inside `buf`, or when PASS A's own walk does not draw it either — every
/// one of those decided BEFORE the delete at [`SpliceRefusal::Untouched`]'s promise.
///
/// [`SpliceRefusal::RegionLost`] when the region-seed walk fails AFTER that delete.
/// This is the one refusal that arrives with the buffer already mutated, and it is a
/// distinct variant rather than the same `None` precisely because the caller's
/// obligation differs: a full re-render is optional after the first and mandatory
/// after the second.
pub(super) fn splice(
    buf: &TextBuffer,
    view: Option<&crate::codeview::CodePreviewView>,
    old_anchored: &[(gtk::TextChildAnchor, gtk::Widget)],
    old_extents: &[crate::renderer::DisclosureExtent],
    prepared: &Prepared<'_>,
    key: FoldKey,
) -> Result<SpliceOutcome, SpliceRefusal> {
    let Some(old_extent) = old_extents.iter().find(|e| e.key == key) else {
        log::debug!(
            "preview::splice: refusing key {key:?} — the previous render recorded no extent \
             for it (an ancestor was collapsed, or it was never drawn); falling back to a \
             full re-render"
        );
        return Err(SpliceRefusal::Untouched);
    };
    // An UNSPACED disclosure's body is literal text inside its own opening raw-HTML
    // event, which a region walk seeded BELOW that event can never write — so the
    // splice would delete the body and put nothing back. Refused here, before the
    // buffer is touched, and the caller's full re-render renders it correctly.
    if !old_extent.spliceable {
        log::debug!(
            "preview::splice: refusing key {key:?} — its body is literal text inside the \
             opening raw-HTML block, which a region render cannot reproduce; falling back \
             to a full re-render"
        );
        return Err(SpliceRefusal::Untouched);
    }
    let old_volatile = old_extent.volatile;
    // The deletion range comes from a PREVIOUS render's extents, and `iter_at_offset`
    // clamps silently — so an offset past the buffer's end would delete to the end of
    // the document and report success. Refused here, before the buffer is touched.
    if old_volatile.start > old_volatile.end || old_volatile.end > buf.char_count() {
        log::error!(
            "preview::splice: refusing key {key:?} — its recorded extent {}..{} is not a \
             range inside a buffer of {} characters; falling back to a full re-render",
            old_volatile.start,
            old_volatile.end,
            buf.char_count()
        );
        return Err(SpliceRefusal::Untouched);
    }

    // PASS A — see module docs. Driven from the SAME `Prepared` as the region walk
    // below, so the "scratch text equals the spliced live text" premise this whole
    // route rests on cannot be broken by the two passes preparing the document
    // differently.
    let products = crate::preview::build::build_products_scratch(prepared);
    let Some(new_extent) = products
        .maps
        .disclosure_extents
        .iter()
        .find(|e| e.key == key)
    else {
        // The doc comment has always promised this one is logged, and it was not.
        // `error` rather than `debug`: PASS A parses the same string with the same
        // options as the walk that produced `old_extents`, so the two disagreeing is a
        // defect somewhere, not a condition the document can produce.
        log::error!(
            "preview::splice: refusing key {key:?} — PASS A's own walk drew no extent for \
             it, though it parsed the same source with the same options as the walk that \
             did; falling back to a full re-render"
        );
        return Err(SpliceRefusal::Untouched);
    };
    let target_collapsed = new_extent.body.is_empty();

    // The live delete, through the trailing newline run. The BOUNDARY is decided by
    // `delete_end_through_newlines` — the one piece of pure arithmetic in this function,
    // and the one whose being wrong is silent: a boundary one character out leaves the
    // buffer a character longer or shorter than PASS A's map describes, which every
    // offset below the splice then inherits.
    let region_end = delete_end_through_newlines(old_volatile.end, |offset| {
        let iter = buf.iter_at_offset(offset);
        (!iter.is_end()).then(|| iter.char())
    });
    let mut del_start = buf.iter_at_offset(old_volatile.start);
    let mut del_end = buf.iter_at_offset(region_end);
    buf.delete(&mut del_start, &mut del_end);

    // PASS SEED+LIVE. The FIRST refusal that can arrive after the buffer has been
    // mutated — see `SpliceRefusal::RegionLost`.
    let Some(region) = render_region(
        buf,
        view,
        prepared,
        key,
        old_volatile.start,
        target_collapsed,
    ) else {
        return Err(SpliceRefusal::RegionLost);
    };

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
        crate::copymap::debug_verify(&products.maps.copymap, &products.maps.md_owned, &chars);
    }

    // ...and the release-safe half of the same question, because the check above is the
    // splice's ONLY correctness oracle and `debug_assertions` compiles it out of exactly
    // the builds users run. PASS A rendered the whole document under the new fold state,
    // so its char count is what this buffer must now hold; a disagreement means the
    // delete boundary or the region write was wrong by that many characters, and the maps
    // about to be installed describe a buffer that does not exist.
    //
    // O(1) against `debug_verify`'s whole-document walk — which is the entire reason a
    // release build can carry it. It is a strictly weaker oracle (it cannot see a
    // same-length error) and is not a replacement for the walk above; it catches the
    // class that actually arises here, which is a length error.
    //
    // Reported as `RegionLost`: the buffer HAS been mutated and is now inconsistent, so
    // the caller's obligation is the same as a failed region write — re-render whole.
    let expected = products.maps.copymap.char_count();
    if buf.char_count() != expected {
        log::error!(
            "preview::splice: after splicing key {key:?} the buffer holds {} characters \
             but PASS A's map describes {expected}; the maps would be installed against a \
             buffer that does not exist, so the pane must be re-rendered whole",
            buf.char_count()
        );
        return Err(SpliceRefusal::RegionLost);
    }

    Ok(SpliceOutcome {
        products: products.into(),
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
fn render_region(
    buf: &TextBuffer,
    view: Option<&crate::codeview::CodePreviewView>,
    prepared: &Prepared<'_>,
    key: FoldKey,
    at_offset: i32,
    target_collapsed: bool,
) -> Option<RegionWidgets> {
    let cleaned = prepared.cleaned();

    // The seed-capture walk (see this function's doc comment). Its buffer gets the
    // same tag table any fresh render would — a body event applies tags as it writes,
    // and an untagged buffer answers every `apply_tag_by_name` with a `Gtk-WARNING`
    // rather than a state difference, but there is no reason to pay the noise.
    let seed_buf = TextBuffer::new(None::<&gtk::TextTagTable>);
    prepared.setup_tags(&seed_buf);
    let mut seed_r = prepared.renderer(seed_buf);

    // The finished region, set by whichever of the two terminations the walk reaches.
    // A `RegionWriter` is consumed by `finish()`, so this cannot hold a region that was
    // never closed — and the `None` below is the one case where PASS A and this walk
    // disagreed, which the caller must treat as a lost region.
    let mut finished: Option<RegionWidgets> = None;
    let mut live: Option<RegionWriter<Writing>> = None;
    for (ev, src) in Parser::new_ext(cleaned, md_options()).into_offset_iter() {
        match live.take() {
            None => {
                // `event_src` must be set before EVERY `process` call — a
                // disclosure's own identity (`DisclosureFrame::key`, minted from
                // `event_src.start` when its opening `<details>` is scanned) is
                // read from it, and `build_products`' own loop sets it the same
                // way for the same reason.
                seed_r.event_src = src.clone();
                seed_r.process(ev);
                if let Some(seed) = seed_r.capture_region_seed(key) {
                    // The whole ordering constraint — the view and the seed before the
                    // offset, the offset before any content, `finish_region` exactly
                    // once at the end — is held by `RegionWriter`'s two states rather
                    // than by the order of the lines below.
                    let mut r = RegionWriter::begin(prepared, buf, view, seed).write_at(at_offset);
                    r.summary_tail(target_collapsed);
                    if target_collapsed {
                        // A collapsed target has no body events to process at all
                        // — `Renderer::process`'s own suppression would swallow
                        // them if it did.
                        finished = Some(r.finish());
                        break;
                    }
                    live = Some(r);
                }
            }
            Some(mut r) => {
                r.process(ev, src);
                if !r.is_open(key) {
                    // The event just processed closed `key`'s own frame — the
                    // region ends here. Everything after this belongs to content
                    // already sitting in the live buffer, untouched; a region
                    // render must never reach past its own block.
                    finished = Some(r.finish());
                    break;
                }
                live = Some(r);
            }
        }
    }
    let Some(region) = finished else {
        // NOT a silent substitution. The caller has ALREADY deleted the region from
        // the live buffer, so writing nothing here would leave the reader's page short
        // by a whole block while every installed map still described the longer
        // buffer. Refuse, and let `splice` report `SpliceRefusal::RegionLost`.
        //
        // Two shapes reach here and both are the same disagreement: the walk never
        // reached `key`'s region at all, or it reached it and the block never closed.
        // A spliceable block is a CLOSED one by construction (`splice` refuses the
        // others before touching the buffer), so neither is reachable while the two
        // walks parse the same string with the same options.
        log::error!(
            "preview::splice: PASS A drew a disclosure_extent for source byte {} but the \
             region-seed walk never produced a finished region for it — the two walks parsed \
             the same string with the same options, so this should be unreachable; the region \
             is LOST and the pane must be re-rendered whole",
            key.source_offset()
        );
        return None;
    };
    Some(region)
}

/// Display-free tests for the delete boundary. Plain `#[cfg(test)]`, not the
/// `gtk-integration-tests` gate the sibling modules carry — that is the point of having
/// extracted it, and it is what puts the rule inside the coverage gate.
#[cfg(test)]
mod boundary_tests {
    use super::delete_end_through_newlines;

    /// The reader: a buffer's characters, answering `None` past the end exactly as
    /// `TextIter::is_end` does.
    fn chars_of(text: &str) -> impl Fn(i32) -> Option<char> + '_ {
        let chars: Vec<char> = text.chars().collect();
        move |offset: i32| {
            usize::try_from(offset)
                .ok()
                .and_then(|i| chars.get(i))
                .copied()
        }
    }

    #[test]
    fn a_region_followed_by_one_newline_takes_it() {
        //            0123 4567
        let text = "abc\ndef";
        assert_eq!(delete_end_through_newlines(3, chars_of(text)), 4);
    }

    #[test]
    fn a_run_of_blank_lines_is_taken_whole() {
        // Three newlines after the region — a block followed by two blank lines. Taking
        // only the first would leave a blank line behind on every toggle.
        let text = "abc\n\n\ndef";
        assert_eq!(delete_end_through_newlines(3, chars_of(text)), 6);
    }

    #[test]
    fn a_region_with_no_trailing_newline_takes_nothing_extra() {
        let text = "abcdef";
        assert_eq!(delete_end_through_newlines(3, chars_of(text)), 3);
    }

    #[test]
    fn a_region_ending_at_the_buffers_end_stops_there() {
        // The edge the live loop needed `is_end()` for: nothing to read past the end, and
        // the boundary must not run away.
        let text = "abc";
        assert_eq!(delete_end_through_newlines(3, chars_of(text)), 3);
    }

    #[test]
    fn a_trailing_newline_run_that_reaches_the_buffers_end_stops_at_it() {
        let text = "abc\n\n";
        assert_eq!(delete_end_through_newlines(3, chars_of(text)), 5);
    }

    #[test]
    fn a_newline_that_is_not_adjacent_is_not_taken() {
        // Only the run IMMEDIATELY after the region counts; a newline further on belongs
        // to whatever follows.
        let text = "abcx\n";
        assert_eq!(delete_end_through_newlines(3, chars_of(text)), 3);
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
