//! The shared experiment harness: the fixture, one arm's reading, and the two ways a
//! toggle can be driven through the splice.
//!
//! Split out of `excursion.rs` at the 500-line soft limit (POLICY § Code style), and
//! the cut was by cause rather than by size — there were several experiments over this
//! same rig, so what they shared stopped being one file's private detail. Only
//! [`super`]'s excursion comparison survives; the others measured GTK's validation
//! budget rather than Scribobulate and were deleted (`git log --
//! src/preview/splice/excursion/`). [`super::rig`] owns establishing a state geometry
//! may legitimately be read from; this file owns the apparatus applied to it; each
//! experiment file owns only its own question.

use gtk::prelude::*;
use std::time::Duration;

use crate::fold::{FoldKey, FoldState};

use super::rig::{Reading, Rig};

/// Pane size. Small enough that a document of the size below towers over it, which is
/// the precondition for the clamp: the collapse only bites when `upper` greatly
/// exceeds `page_size`.
pub(super) const PANE_W: i32 = 700;
pub(super) const PANE_H: i32 = 600;

/// Zoom 1.0 throughout — this measures the adjustment, not the zoom path.
pub(super) const ZOOM: f64 = 1.0;

/// Filler wide enough to wrap several times at [`PANE_W`], so each paragraph
/// contributes real height rather than one line.
pub(super) const FILLER: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do \
     eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim \
     veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo.";

/// Paragraphs inside the disclosure body. Enough that expanding it adds real height
/// ABOVE the reading position — a body that fits inside the collapsed preview would
/// render to nearly the same height either way and measure nothing.
///
/// A FLOOR rather than the count: the body carries at most one anchored child per
/// paragraph (they are spread, never clustered — see
/// [`tall_document_with_body_and_tail_children`]), so a dose larger than this grows the
/// body to hold it. Every dose at or below 30 therefore renders exactly the body every
/// table in this directory was measured against, unchanged.
pub(super) const BODY_PARAS: usize = 30;

/// Paragraphs after the disclosure. Enough that the settled `upper` is tens of
/// thousands of pixels against a 600px viewport, matching the scale the plan measured
/// the defect at (~28 000 px on a 107 KB document).
pub(super) const TAIL_PARAS: usize = 500;

/// Where the reader is parked, as a fraction of the scrollable range. Well down the
/// document, so "thrown back to the top" and "stayed put" are unmistakably different
/// readings — a position near the top makes the two outcomes agree and the
/// measurement vacuous.
pub(super) const READING_FRACTION: f64 = 0.6;

/// How long `upper` must hold still before the layout counts as settled.
///
/// A DURATION rather than a turn count, per [`testpump::until_stable`]: "N equal
/// samples" is satisfiable in microseconds by a loop that spins faster than the work
/// it waits on.
pub(super) const QUIET: Duration = Duration::from_millis(300);

/// Generous failure bound for every settle here (GTK4Rs/AP-122 — a bound, never the
/// completion signal).
pub(super) const SETTLE_DEADLINE: Duration = Duration::from_secs(30);

/// The document: one collapsed disclosure near the top, then a long tail the reader
/// scrolls down into.
///
/// **Deliberately knob-free.** It grew four layers of parameters — a body dose of
/// anchored children, an independent tail dose, and a body filler length — for the
/// seven characterization arms that varied them. Those are gone (see [`super`]'s module
/// docs for where their conclusion lives), and a fixture builder whose knobs nothing
/// turns is a fixture nobody can tell is still correct.
pub(super) fn tall_document() -> String {
    let mut md = String::from(
        "# Measurement fixture\n\nAn opening paragraph, before the disclosure.\n\n\
         <details>\n<summary>A collapsible section</summary>\n\n",
    );
    for i in 0..BODY_PARAS {
        md.push_str(&format!("Hidden body paragraph {i}. {FILLER}\n\n"));
    }
    md.push_str("</details>\n\n");
    for i in 0..TAIL_PARAS {
        md.push_str(&format!("Tail paragraph {i}. {FILLER}\n\n"));
    }
    md
}

/// Everything one arm of the experiment measured.
pub(super) struct Arm {
    /// The name printed in the report.
    pub(super) route: &'static str,
    /// Settled, after the reader scrolled down but before the toggle.
    pub(super) before: Reading,
    /// The same turn the toggle returned in, with nothing pumped.
    pub(super) immediate: Reading,
    /// The lowest `value` seen at any point during the settle that followed.
    pub(super) min_value: f64,
    /// The lowest `upper` seen at any point during the settle that followed.
    pub(super) min_upper: f64,
    /// Settled, after the toggle.
    pub(super) after: Reading,
    /// The text of the line at the top of the viewport, before and after — the
    /// "same content?" half of TDD 2.26h, which the adjustment numbers cannot answer.
    pub(super) top_before: String,
    pub(super) top_after: String,
    /// The line the reader was looking at, tracked by a `GtkTextMark` placed at the
    /// top of the viewport before the toggle, and its text after it — so a route that
    /// destroyed the reader's anchor can be told from one that merely moved it.
    pub(super) anchor_text_after: String,
    /// How far the reader's marked line sits BELOW the top of the viewport, before
    /// and after. Zero drift between the two is TDD 2.26h's "same content" exactly;
    /// see [`Arm::content_drift_px`].
    pub(super) anchor_offset_before: f64,
    pub(super) anchor_offset_after: f64,
}

impl Arm {
    /// Did `upper` collapse — did the document's known height fall to something near
    /// the viewport, rather than merely moving by the toggled block's own height?
    ///
    /// The threshold is deliberately loose (a HALVING) rather than pinned near the
    /// documented ~650 px: what distinguishes the two routes is a collapse to roughly
    /// the viewport versus no collapse at all, and a tight threshold would turn a
    /// font-metric difference between hosts into a failure (the same portability trap
    /// `farscroll`'s own settle helper records).
    pub(super) fn upper_collapsed(&self) -> bool {
        self.min_upper < self.before.upper / 2.0
    }

    /// Was the reader thrown to the top — did `value` fall from a position tens of
    /// thousands of pixels down to within one screenful of the document's start?
    ///
    /// "Within a screenful of zero" rather than `== 0.0`: MEASURED, the clamp leaves
    /// `value` at 2 rather than at 0 on this rig, and a literal zero test would read
    /// a full collapse as no collapse at all. What the rubric forbids is the reader
    /// being thrown back to the top, and two pixels from the top is the top.
    pub(super) fn thrown_to_the_top(&self) -> bool {
        self.min_value <= self.before.page_size
    }

    /// **How far the reader's content moved on screen**, in pixels.
    ///
    /// The adjustment numbers cannot answer TDD 2.26h on their own: a toggle above
    /// the viewport changes the document's height, so an unchanged `value` means the
    /// reader has moved *relative to the content* by exactly the height inserted
    /// above them. This tracks the reader's own line through a `GtkTextMark` instead
    /// and reports the change in its distance below the top of the viewport. Zero is
    /// "stayed on the same content"; positive means the content slid down the screen,
    /// negative that it slid up.
    ///
    /// Meaningless unless [`Self::anchor_survived`] — a route that rebuilt the whole
    /// buffer collapsed every mark to offset 0, so the number would describe the top
    /// of the document rather than the reader.
    pub(super) fn content_drift_px(&self) -> f64 {
        self.anchor_offset_after - self.anchor_offset_before
    }

    /// Did the reader's anchor survive the route at all? A full re-render deletes
    /// every character, and GTK moves a mark inside a deleted range to the deletion
    /// point rather than deleting it — so `is_deleted()` says `false` for an anchor
    /// that now names the top of the document. The text is the honest test.
    pub(super) fn anchor_survived(&self) -> bool {
        self.anchor_text_after == self.top_before
    }

    pub(super) fn report(&self) -> String {
        format!(
            "  {route:<10} before    {before}\n\
             \x20            immediate {immediate}\n\
             \x20            TROUGH    value {min_value:>9.0}  upper {min_upper:>9.0}\n\
             \x20            settled   {after}\n\
             \x20            delta     value {dvalue:>+9.0}  upper {dupper:>+9.0}\n\
             \x20            upper collapsed: {collapsed:<5}  thrown to the top: {thrown}\n\
             \x20            reader anchor survived: {survived:<5}  content drift: \
             {drift}\n\
             \x20            top line before: {top_before:?}\n\
             \x20            top line after:  {top_after:?}\n",
            route = self.route,
            before = self.before,
            immediate = self.immediate,
            min_value = self.min_value,
            min_upper = self.min_upper,
            after = self.after,
            dvalue = self.after.value - self.before.value,
            dupper = self.after.upper - self.before.upper,
            collapsed = self.upper_collapsed(),
            thrown = self.thrown_to_the_top(),
            survived = self.anchor_survived(),
            drift = if self.anchor_survived() {
                format!("{:+.0}px", self.content_drift_px())
            } else {
                "n/a (anchor destroyed)".to_string()
            },
            top_before = truncate(&self.top_before),
            top_after = truncate(&self.top_after),
        )
    }
}

/// Keep the report readable — a fixture paragraph is 200-odd characters.
pub(super) fn truncate(s: &str) -> String {
    s.chars().take(46).collect()
}

/// Run one arm end to end: build, settle, scroll down, toggle, measure.
///
/// `toggle` is handed the rig with `folds` ALREADY reflecting the new state, and does
/// whatever that route does to the live buffer. Everything around it — fixture,
/// window size, settle discipline, reading position — is identical between arms, so
/// the route is the only variable.
///
/// **This was three functions and an enum until the excursion arms that used them were
/// deleted.** `measure_probed` offered an instrument at two `Phase`s and
/// `measure_probed_at_margin` threaded a per-view `top-margin`; both existed for the
/// margin sweep and the per-emission trace, which measured GTK's validation budget
/// rather than Scribobulate and were removed with it (`git log --
/// src/preview/splice/excursion/`). What survived was a parameterisation nothing
/// turned and an assertion nothing could reach, documented as though a caller might
/// (F-TEST-A-005).
pub(super) fn measure(
    route: &'static str,
    md: &str,
    start_expanded: bool,
    toggle: impl FnOnce(&Rig, &FoldState, FoldKey),
) -> Arm {
    let spans = crate::renderer::disclosure::scan_document(md);
    let key = spans[0].fold_key();

    // The fixture's `<details>` carries no `open`, so the default state draws it
    // COLLAPSED and one toggle expands it. Starting expanded is therefore one toggle
    // ahead, and the measured toggle then collapses it — the other direction of
    // TDD 2.26h, where the document SHRINKS above the reader.
    let mut start = FoldState::default();
    if start_expanded {
        start.toggle(key);
    }

    let rig = Rig::new(md, &start);
    let adjustment = rig.adjustment();
    assert!(
        adjustment.upper() > adjustment.page_size() * 8.0,
        "precondition: the fixture must tower over the viewport for the clamp to be \
         reachable at all — upper {:.0} against page {:.0}. A shorter document cannot \
         exhibit the collapse and every number below would be meaningless.",
        adjustment.upper(),
        adjustment.page_size(),
    );

    rig.scroll_to_reading_position();
    let before = Reading::of(&adjustment);
    let top_before = rig.top_line_text();
    let anchor = rig.anchor_reader();
    let (anchor_offset_before, _) = rig.reader_offset(&anchor);
    assert!(
        before.value > before.page_size,
        "precondition: the reader must actually be parked well below the top, or \
         'thrown to the top' and 'stayed put' are the same reading"
    );

    // The reader toggles the block. `folds` reflects the NEW state, as every caller
    // of `splice` must arrange.
    let mut folds = start.clone();
    folds.toggle(key);
    toggle(&rig, &folds, key);

    let immediate = Reading::of(&adjustment);
    let (min_value, min_upper) = rig.settle_watching_the_trough();
    let after = Reading::of(&adjustment);
    let top_after = rig.top_line_text();
    let (anchor_offset_after, anchor_text_after) = rig.reader_offset(&anchor);
    rig.teardown();

    Arm {
        route,
        before,
        immediate,
        min_value,
        min_upper,
        after,
        top_before,
        top_after,
        anchor_text_after,
        anchor_offset_before,
        anchor_offset_after,
    }
}

/// Run the SPLICE route's toggle against `rig`, exactly as production does it: the
/// view is handed to the region renderer, so each of its fresh children is parented in
/// the same turn as its anchor (`Renderer::push_anchored`).
///
/// The survivors are already parented and must not be handed to `attach_anchored`
/// again (the module docs name that as the caller's obligation); only the region
/// render's own outputs are new, and the region renderer has already taken them.
///
/// **Returns nothing.** It used to hand back the REGION render's own child count so a
/// caller measuring a per-child effect could check the dose arrived; the only caller
/// that did was the dose-response sweep, deleted with the other experiments that
/// measured GTK rather than Scribobulate (`git log --
/// src/preview/splice/excursion/`). The count was worth having for that question and
/// is worth nothing to this one.
pub(super) fn splice_toggle(rig: &Rig, folds: &FoldState, key: FoldKey, md: &str) {
    let outcome = crate::preview::splice::splice(
        &rig.buf,
        Some(&rig.view),
        &rig.anchored,
        &rig.extents,
        &crate::preview::build::Prepared::new(md, None, ZOOM, false, crate::theme::active(), folds),
        key,
    )
    .expect("the toggled block was drawn in the starting render");

    // Prove the parenting really happened before anything downstream grades a number
    // against it (ScrAP-252's family: a SETUP step that silently fails to take effect
    // makes the next assertion answer for the previous state).
    //
    // **Vacuous when the region drew no child at all**, and deliberately left that way:
    // a collapsed region draws none and an expanded one draws no toggle button either
    // (the summary line, and so the toggle, belongs to the seed walk that runs before
    // the region begins), so requiring a non-empty list here would fail on correct
    // renders. The caller that could tell a real dose from an empty one was the
    // deleted dose-response sweep.
    assert!(
        outcome
            .region
            .anchored
            .iter()
            .all(|(_, w)| w.parent().is_some()),
        "the region render was handed the view and asked to parent its {} children as \
         it created them, and at least one came back unparented",
        outcome.region.anchored.len(),
    );
}
