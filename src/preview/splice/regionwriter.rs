//! **The region render's method sequence, made unrepresentable in the wrong order.**
//!
//! A live-writing [`Renderer`] must be handed its view BEFORE `write_at` (so every
//! anchored child is parented in the same turn as its anchor), seeded before
//! `write_at`, written-at before its summary tail, tailed before its first event, and
//! finished exactly once at the end of the region. Each of those is an ordinary
//! `&mut self` method on `Renderer`,
//! callable in any order; until this type existed, the only thing holding the sequence
//! was that one function happened to call them in it, and a second caller had nothing
//! to consult but prose.
//!
//! **Why that is worth a type.** Getting one of them wrong is not a crash. A
//! left-gravity write mark writes every run at the same place and reverses the
//! document. A summary tail written into the wrong buffer leaves a seed that is
//! logically correct while the buffer it is replayed into is physically missing the
//! very byte that state describes. Both produce a *well-formed* buffer with every map
//! below the splice silently offset — the failure class ScrAP-148 records for exactly
//! this kind of in-place write, and one that no assertion downstream of the render can
//! attribute back to it.
//!
//! The typestate is deliberately small: three states on one linear path, each
//! transition consuming the writer. `Ready` cannot write content; `Aimed` can do
//! nothing but write the summary tail; `Writing` cannot be re-seeded, re-pointed or
//! re-tailed; and the region is finished by [`RegionWriter::finish`], which consumes
//! the writer, so `finish_region` runs exactly once and its outputs can only be read
//! after it has.

use super::RegionWidgets;
use crate::codeview::CodePreviewView;
use crate::fold::FoldKey;
use crate::preview::build::Prepared;
use crate::renderer::{RegionSeed, Renderer};
use gtk::prelude::*;
use gtk::TextBuffer;
use std::marker::PhantomData;

/// Seeded and pointed at its view, but not yet aimed at a buffer offset. Cannot write.
pub(super) struct Ready;
/// Aimed at the region's start offset, and owing the summary line's tail. The only
/// thing it can do is write that tail, which is the only way on.
pub(super) struct Aimed;
/// The tail is written. The only state that processes events, and the only one that
/// can be finished.
pub(super) struct Writing;

/// A [`Renderer`] driving one region write, in a state that says what it may do next.
pub(super) struct RegionWriter<S> {
    r: Renderer,
    /// The live view, when there is one. Held so [`Self::finish`] can issue the batched
    /// `queue_resize` the two-step attach route would otherwise have issued; a bare
    /// buffer caller (`super::tests`) has none.
    view: Option<CodePreviewView>,
    _state: PhantomData<S>,
}

impl RegionWriter<Ready> {
    /// The LIVE renderer for a region: constructed from the same [`Prepared`] document
    /// PASS A used, pointed at `view` and seeded from the scratch walk — all before any
    /// offset is chosen, which is the whole ordering constraint this type exists for.
    ///
    /// `view` is `Some` on the production path, where each anchored child is parented
    /// in the same turn as its anchor (see `Renderer::push_anchored`). `None` keeps the
    /// two-step attach behaviour a bare-buffer caller needs.
    pub(super) fn begin(
        prepared: &Prepared<'_>,
        buf: &TextBuffer,
        view: Option<&CodePreviewView>,
        seed: RegionSeed,
    ) -> Self {
        let mut r = prepared.renderer(buf.clone());
        if let Some(view) = view {
            r.set_live_view(view);
        }
        r.seed(seed);
        Self {
            r,
            view: view.cloned(),
            _state: PhantomData,
        }
    }

    /// Aim the write at `offset` — the region's start, which the caller has already
    /// emptied. The one transition out of [`Ready`], so nothing can emit content
    /// against the buffer end by accident.
    pub(super) fn write_at(mut self, offset: i32) -> RegionWriter<Aimed> {
        self.r.write_at(offset);
        RegionWriter {
            r: self.r,
            view: self.view,
            _state: PhantomData,
        }
    }
}

impl RegionWriter<Aimed> {
    /// The summary line's own tail: the collapsed-body preview (if `target_collapsed`),
    /// then the newline `emit_pending_summary` always writes next.
    ///
    /// A catch-up write, not double bookkeeping — the scratch walk emitted this into
    /// ITS buffer during seed capture, never into the live one. See
    /// `Renderer::write_seeded_summary_tail`.
    ///
    /// **The one transition into [`Writing`], and it CONSUMES the writer** — so the
    /// tail is written before the first event and cannot be written twice. It was an
    /// `&mut self` method on `Writing`, which the typestate's own doc claimed to order
    /// and did not: a caller could process the whole region and then write the tail
    /// into the middle of it, or write it twice, and either produces a well-formed
    /// buffer with every map below the splice offset (F-AP2-009).
    pub(super) fn summary_tail(mut self, target_collapsed: bool) -> RegionWriter<Writing> {
        self.r.write_seeded_summary_tail(target_collapsed);
        RegionWriter {
            r: self.r,
            view: self.view,
            _state: PhantomData,
        }
    }
}

impl RegionWriter<Writing> {
    /// Process one parse event into the region.
    ///
    /// `event_src` is set here rather than by the caller because it must be set before
    /// EVERY `process` call: a disclosure's identity (`DisclosureFrame::key`, minted
    /// from `event_src.start` when its opening `<details>` is scanned) is read from it,
    /// and a caller that sets it separately can set it for some events and not others.
    pub(super) fn process(&mut self, ev: pulldown_cmark::Event<'_>, src: std::ops::Range<usize>) {
        self.r.event_src = src;
        self.r.process(ev);
    }

    /// Is the region's own block still open? Once `key`'s frame has popped, the block
    /// and everything nested in it has closed and nothing after it belongs to this
    /// region.
    pub(super) fn is_open(&self, key: FoldKey) -> bool {
        self.r.is_open(key)
    }

    /// Close the region and hand back the widgets it created.
    ///
    /// Consumes the writer, so `finish_region` runs exactly once and the outputs are
    /// unreachable before it has. Only the region's OWN children are here — never a
    /// survivor of the caller's delete.
    pub(super) fn finish(mut self) -> RegionWidgets {
        self.r.finish_region();
        // `attach_anchored`'s own trailing `queue_resize`, which the eager path would
        // otherwise skip: parenting N children queues N resizes on the view, but the
        // batched one is what the two-step route does and the arms must differ only in
        // WHEN the children are parented.
        if let Some(view) = &self.view {
            if !self.r.anchored.is_empty() {
                view.queue_resize();
            }
        }
        // The take lives on `Renderer`, beside the fields it empties — see
        // `Renderer::take_region_widgets`. Six `mem::take` calls here were a
        // non-exhaustive read of another module's struct, and a seventh list would have
        // joined it by nobody thinking to (F-TEST-A-008).
        self.r.take_region_widgets()
    }
}
