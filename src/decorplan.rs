//! **The preview's paint plan — what `CodePreviewView::snapshot_layer` draws, in
//! what order, and every decision it takes that does not need a display.**
//!
//! The paint itself stays in `codeview`, where the `GtkSnapshot` and the coordinate
//! conversions live. What moved here is the part that is *arithmetic and precedence*:
//! the ordered list of steps, the visibility gates, the corner-radius clamp, the
//! reveal rule for a code block's copy button, and the state machine behind a
//! programmatic navigation's pending popover. The same split `affordance` makes with
//! `codeview::copybutton` and `keynav` makes with `codeview::navkeys` — and the split
//! POLICY's coverage scope rule asks for by name, since `codeview/` is excluded from
//! the gate and logic left inside it is logic nothing measures.
//!
//! **[`PAINT_ORDER`] is the compositing order, and it is DATA the paint iterates**
//! rather than the sequence its statements happen to be written in. That is the whole
//! reason it is here. The order between two decorations that overlap — a heading band
//! inside a blockquote must land ON the panel, a marker sprite over the list
//! background — is a real property of the rendering and it used to be expressible only
//! as "these two `append_color` calls are 150 lines apart, in this order". Nothing
//! could assert it without painting pixels, and rearranging the function could not help
//! but put it at risk. As a table it is one `assert_eq!` away, and a decomposition that
//! reorders the steps has to say so in a diff a reviewer reads.
//!
//! It does not REPLACE the pixel guards in `codeview::ordertests` — those prove the
//! painted result and this proves the intent, and only the pair catches both a table
//! that says the wrong thing and a painter that ignores what it says.

use gtk::TextViewLayer;

/// One step of the preview's decoration paint.
///
/// Every variant either draws something or depends on what an earlier one drew, which
/// is why the pending-popover dispatch is in the list rather than tacked on after it:
/// it consumes the hit-boxes [`Self::AnnotationChip`] records, so its position is a
/// real constraint and not a formatting choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaintStep {
    /// A blockquote's full-width fill (TDD 18.29). First, because a quote is the
    /// outermost container in the vocabulary: every decoration a quote can hold must
    /// land ON the panel rather than under it.
    QuotePanel,
    /// The band behind a heading (TDD 18.25) — before every per-block decoration
    /// except the panel, for the same containment reason.
    HeadingBand,
    /// A fenced code block's card, and the rectangles the copy button is placed from.
    CodeCard,
    /// A blockquote's accent bar. After the panel it shares an extent with, and after
    /// the band and the card, because a quoted heading or code block runs UNDER the
    /// bar rather than over it.
    QuoteBar,
    /// The drawn list-item markers, and the task checkboxes' hit-boxes.
    ListGutter,
    /// The right-margin annotation chips, and the hit-boxes a click maps through.
    /// Above the text so a chip on a table cell's row is not hidden by the opaque
    /// anchored table (the "cell markers don't show" defect).
    AnnotationChip,
    /// A code block's copy button. Above the text because it sits in the card's
    /// top-right corner and a long first line runs underneath it.
    CopyButton,
    /// Fire a programmatic navigation's armed open-request, now that the chips have
    /// recorded this frame's hit-boxes. Not a decoration — the paint's own completion
    /// event, which GTK offers no signal for.
    PendingOpen,
}

impl PaintStep {
    /// Which of `GtkTextView`'s two paint passes this step belongs to.
    ///
    /// The layer is what orders the below-text steps against the above-text ones, and
    /// GTK owns it: no rearrangement within a pass can put a card over its own copy
    /// button, and moving a step across passes is the only way to break that pair.
    pub(crate) fn layer(self) -> TextViewLayer {
        match self {
            PaintStep::QuotePanel
            | PaintStep::HeadingBand
            | PaintStep::CodeCard
            | PaintStep::QuoteBar
            | PaintStep::ListGutter => TextViewLayer::BelowText,
            PaintStep::AnnotationChip | PaintStep::CopyButton | PaintStep::PendingOpen => {
                TextViewLayer::AboveText
            }
        }
    }
}

/// Every step of the preview's paint, in compositing order — the single place that
/// order is written down, and the sequence `snapshot_layer` iterates.
pub(crate) const PAINT_ORDER: &[PaintStep] = &[
    PaintStep::QuotePanel,
    PaintStep::HeadingBand,
    PaintStep::CodeCard,
    PaintStep::QuoteBar,
    PaintStep::ListGutter,
    PaintStep::AnnotationChip,
    PaintStep::CopyButton,
    PaintStep::PendingOpen,
];

/// Whether a decoration measured to buffer-space rows `[y, y + h]` is on screen.
///
/// The straddle clamp every per-row decoration applies after refining its y: an
/// annotation chip on a table row, or a list marker whose height was clamped to its
/// first display line, can leave the viewport while the offset it was found by is
/// still inside it. Reading an off-screen line's geometry is the ScrAP-22 hazard the
/// whole paint is arranged to avoid, so the gate is not an optimisation.
pub(crate) fn row_on_screen(y: f32, h: f32, vtop: f32, vbot: f32) -> bool {
    y + h >= vtop && y <= vbot
}

/// Whether a decoration anchored at a single buffer offset is on screen.
///
/// The cheap offset gate the row gate above refines. Inclusive at both ends: an
/// offset exactly on the last visible line is visible.
pub(crate) fn offset_on_screen(offset: i32, vis_start: i32, vis_end: i32) -> bool {
    offset >= vis_start && offset <= vis_end
}

/// The corner radius to round a heading band's rectangle by.
///
/// `design_px` is the theme's design-time value at zoom 1.0 — pixel metrics are widget
/// properties and do not follow the CSS `font-size` rule, so they are scaled here
/// explicitly on every render (POLICY "No hard-coded styling"). The two clamps stop a
/// theme stating a radius larger than the band from producing a degenerate rounded
/// rectangle: at half the width or half the height the corners already meet, and GSK
/// is handed no rounding it cannot draw.
pub(crate) fn band_corner_radius(design_px: i32, zoom: f64, w: f32, h: f32) -> f32 {
    let scaled = (f64::from(design_px) * zoom).round() as f32;
    if scaled <= 0.0 {
        return 0.0;
    }
    scaled.min(w / 2.0).min(h / 2.0)
}

/// The theme slot a heading of `level_index` reads its band from.
///
/// Redundant by construction since `level_index` became a `theme::heading_slot`
/// result, which cannot exceed the bound — kept because the alternative on the paint
/// path is an index panic inside a `snapshot` callback, and a defence costing one
/// comparison per visible heading is cheaper than that. Stated as redundant rather
/// than left to read as load-bearing (GTK4Rs/AP-254).
pub(crate) fn band_slot(level_index: usize, levels: usize) -> usize {
    level_index.min(levels.saturating_sub(1))
}

/// Whether block `index`'s copy button is drawn this paint.
///
/// Two states the reader can be in, and they are deliberately separate inputs:
/// `hovered` is the block under the pointer, `copied` the one whose confirmation is
/// still showing, so the checkmark survives the pointer leaving.
pub(crate) fn copy_button_shown(
    index: usize,
    hovered: Option<usize>,
    copied: Option<usize>,
) -> bool {
    hovered == Some(index) || copied == Some(index)
}

/// Whether any copy button can be drawn at all, so the paint can skip the per-block
/// work when none can.
pub(crate) fn any_copy_button_shown(hovered: Option<usize>, copied: Option<usize>) -> bool {
    hovered.is_some() || copied.is_some()
}

/// What the paint should do about a programmatic navigation's armed open-request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PendingOpenGate {
    /// Nothing is armed — do not touch the request or the hit-boxes.
    Idle,
    /// The wall-clock budget ran out. Clear the request; the document keeps whatever
    /// position the converge loop left it at, which is the visible give-up.
    Abandon,
    /// Ask the hit-boxes whether the target chip painted this frame. A hit dispatches
    /// and clears the request; a miss leaves it armed and the converge loop keeps
    /// aiming.
    Consult,
}

/// The precedence behind that decision, which is the part worth pinning.
///
/// **Expiry is tested BEFORE the scroll, and the scroll before the paint**, and both
/// orderings are deliberate:
///
/// * A paint arriving after the budget is by construction not the paint this
///   navigation caused, so opening on it would throw a surprise popover at a rect the
///   reader is no longer asking about — they have scrolled elsewhere and the chip
///   merely happens to be on screen again. Expiry wins over a late hit.
/// * A chip can become visible while lazy line-height validation is still growing the
///   adjustment's `upper`, so dispatching on the paint alone freezes the scroll at
///   whatever partial position revealed it (measured on GDK-Win32: left at 103 against
///   a reachable 263). Requiring the scroll to have landed first is what closes that,
///   and it is a test about the state already in front of us rather than a flag some
///   future tick must set — ScrAP-202, where gating on the converge loop's own
///   completion silenced the very paint the dispatch rides on.
///
/// `landed` is a closure rather than a `bool` so the precedence stays a property of
/// this function instead of leaking back to the call site as a short-circuit there.
/// Answering it costs the caller a geometry read against the live adjustment, and the
/// expired branch must not pay for one — nor take one on a frame where the answer is
/// discarded.
pub(crate) fn pending_open_gate(
    armed: bool,
    expired: bool,
    landed: impl FnOnce() -> bool,
) -> PendingOpenGate {
    if !armed {
        PendingOpenGate::Idle
    } else if expired {
        PendingOpenGate::Abandon
    } else if !landed() {
        PendingOpenGate::Idle
    } else {
        PendingOpenGate::Consult
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_paint_step_appears_exactly_once_in_the_order() {
        // The list is what the paint iterates, so a step missing from it is a
        // decoration that silently stops being drawn and one listed twice is a
        // decoration painted over itself.
        let all = [
            PaintStep::QuotePanel,
            PaintStep::HeadingBand,
            PaintStep::CodeCard,
            PaintStep::QuoteBar,
            PaintStep::ListGutter,
            PaintStep::AnnotationChip,
            PaintStep::CopyButton,
            PaintStep::PendingOpen,
        ];
        assert_eq!(PAINT_ORDER.len(), all.len());
        for step in all {
            assert_eq!(
                PAINT_ORDER.iter().filter(|s| **s == step).count(),
                1,
                "{step:?} must appear exactly once in PAINT_ORDER"
            );
        }
    }

    /// Position of `step` in the compositing order.
    fn at(step: PaintStep) -> usize {
        PAINT_ORDER
            .iter()
            .position(|s| *s == step)
            .expect("every step is in the order")
    }

    #[test]
    fn a_quote_panel_is_painted_under_everything_a_quote_can_contain() {
        // A blockquote is the outermost container in the vocabulary — it can hold a
        // heading, a code block, a list, and its own accent bar — so its fill is the
        // ground all four are drawn against. This is the pair `codeview::ordertests`
        // drives in pixels; here it is the intent behind them.
        for inside in [
            PaintStep::HeadingBand,
            PaintStep::CodeCard,
            PaintStep::QuoteBar,
            PaintStep::ListGutter,
        ] {
            assert!(
                at(PaintStep::QuotePanel) < at(inside),
                "{inside:?} must be painted after the quote panel it can sit inside"
            );
        }
    }

    #[test]
    fn the_accent_bar_is_painted_over_the_block_decorations_it_runs_through() {
        // The bar runs the whole quote, including the rows a quoted heading band or
        // code card covers, and it must stay visible across them.
        assert!(at(PaintStep::HeadingBand) < at(PaintStep::QuoteBar));
        assert!(at(PaintStep::CodeCard) < at(PaintStep::QuoteBar));
    }

    #[test]
    fn list_markers_are_painted_over_the_block_decorations_that_share_their_row() {
        // `- # Heading` and an item whose first line is a fence both put a marker
        // inside a full-content-column rectangle, measured against the renderer.
        assert!(at(PaintStep::HeadingBand) < at(PaintStep::ListGutter));
        assert!(at(PaintStep::CodeCard) < at(PaintStep::ListGutter));
    }

    #[test]
    fn the_copy_button_is_painted_in_the_pass_after_its_card() {
        // The one ordering the layer owns rather than the list: the card fills the
        // rectangle the button sits in, and the button also READS the rectangles the
        // card step records, so both reasons point the same way.
        assert_eq!(PaintStep::CodeCard.layer(), TextViewLayer::BelowText);
        assert_eq!(PaintStep::CopyButton.layer(), TextViewLayer::AboveText);
    }

    #[test]
    fn the_pending_open_dispatch_runs_after_the_chips_it_reads() {
        // It is the paint's self-generated completion event, and what it consults is
        // the hit-box table the chip step clears and repopulates — so running it first
        // would answer from the PREVIOUS frame.
        assert!(at(PaintStep::AnnotationChip) < at(PaintStep::PendingOpen));
        assert_eq!(
            PaintStep::AnnotationChip.layer(),
            PaintStep::PendingOpen.layer()
        );
    }

    #[test]
    fn every_below_text_step_precedes_every_above_text_one() {
        // Not a taste rule: `snapshot_layer` runs the list twice, once per pass, and
        // filters by layer. An interleaved list would still paint correctly, but the
        // order as READ would no longer be the order as PAINTED, which is exactly the
        // gap this table exists to close.
        let first_above = PAINT_ORDER
            .iter()
            .position(|s| s.layer() == TextViewLayer::AboveText)
            .expect("some step paints above the text");
        assert!(PAINT_ORDER[..first_above]
            .iter()
            .all(|s| s.layer() == TextViewLayer::BelowText));
        assert!(PAINT_ORDER[first_above..]
            .iter()
            .all(|s| s.layer() == TextViewLayer::AboveText));
    }

    #[test]
    fn a_row_straddling_either_viewport_edge_is_still_on_screen() {
        // Partly visible is visible — clamping the RECT is the paint's discipline,
        // culling the row is not.
        assert!(row_on_screen(-5.0, 20.0, 0.0, 100.0));
        assert!(row_on_screen(95.0, 20.0, 0.0, 100.0));
        assert!(row_on_screen(0.0, 20.0, 0.0, 100.0));
    }

    #[test]
    fn a_row_wholly_past_either_edge_is_off_screen() {
        assert!(!row_on_screen(-30.0, 20.0, 0.0, 100.0));
        assert!(!row_on_screen(101.0, 20.0, 0.0, 100.0));
    }

    #[test]
    fn an_offset_on_either_visible_boundary_counts_as_visible() {
        assert!(offset_on_screen(10, 10, 40));
        assert!(offset_on_screen(40, 10, 40));
        assert!(!offset_on_screen(9, 10, 40));
        assert!(!offset_on_screen(41, 10, 40));
    }

    #[test]
    fn a_band_radius_scales_with_zoom_and_rounds_to_a_pixel() {
        assert_eq!(band_corner_radius(6, 1.0, 400.0, 40.0), 6.0);
        assert_eq!(band_corner_radius(6, 2.0, 400.0, 40.0), 12.0);
        // 6 * 1.5 = 9.0 exactly; 5 * 1.5 = 7.5 rounds away from zero.
        assert_eq!(band_corner_radius(5, 1.5, 400.0, 40.0), 8.0);
    }

    #[test]
    fn a_band_radius_never_exceeds_half_the_rectangle_it_rounds() {
        // A theme may state any integer; the band is whatever the document makes it.
        assert_eq!(band_corner_radius(500, 1.0, 40.0, 400.0), 20.0);
        assert_eq!(band_corner_radius(500, 1.0, 400.0, 30.0), 15.0);
    }

    #[test]
    fn an_unstated_or_zeroed_band_radius_asks_for_no_rounding_at_all() {
        // The caller reads 0.0 as "square corners, push no clip" — so a metric that
        // scales to nothing must not come back as a hairline radius.
        assert_eq!(band_corner_radius(0, 3.0, 400.0, 40.0), 0.0);
        assert_eq!(band_corner_radius(-4, 1.0, 400.0, 40.0), 0.0);
    }

    #[test]
    fn a_copy_button_shows_for_the_hovered_block_and_for_the_copied_one() {
        assert!(copy_button_shown(2, Some(2), None));
        assert!(copy_button_shown(2, None, Some(2)));
        assert!(copy_button_shown(2, Some(5), Some(2)));
        assert!(!copy_button_shown(2, Some(5), Some(7)));
        assert!(!copy_button_shown(2, None, None));
    }

    #[test]
    fn the_copy_button_pass_is_skipped_only_when_neither_state_holds() {
        assert!(!any_copy_button_shown(None, None));
        assert!(any_copy_button_shown(Some(0), None));
        assert!(any_copy_button_shown(None, Some(0)));
    }

    #[test]
    fn an_unarmed_pending_open_touches_nothing() {
        // Including when the flags around it would otherwise say abandon: with no
        // request there is nothing to clear, and clearing is a write.
        assert_eq!(
            pending_open_gate(false, true, || true),
            PendingOpenGate::Idle,
            "an expired flag with no request armed must not be read as an abandon"
        );
        assert_eq!(
            pending_open_gate(false, false, || false),
            PendingOpenGate::Idle
        );
    }

    #[test]
    fn expiry_beats_a_landed_scroll_and_a_painted_chip() {
        // The precedence, and the reason it is a function rather than a comment: a
        // paint after the budget is not the paint this navigation caused.
        assert_eq!(
            pending_open_gate(true, true, || true),
            PendingOpenGate::Abandon
        );
        assert_eq!(
            pending_open_gate(true, true, || false),
            PendingOpenGate::Abandon
        );
    }

    #[test]
    fn a_chip_that_has_painted_before_the_scroll_landed_keeps_waiting() {
        // The GDK-Win32 case: the chip surfaces earlier there relative to validation,
        // and dispatching on it froze the scroll part-way. Staying armed costs
        // nothing — the converge loop keeps aiming and the deadline still bounds it.
        assert_eq!(
            pending_open_gate(true, false, || false),
            PendingOpenGate::Idle
        );
    }

    #[test]
    fn an_armed_landed_request_consults_the_hit_boxes() {
        assert_eq!(
            pending_open_gate(true, false, || true),
            PendingOpenGate::Consult
        );
    }

    #[test]
    fn neither_the_expired_nor_the_unarmed_branch_asks_whether_the_scroll_landed() {
        // The short-circuit is a property of THIS function, not of its caller, and it
        // is the reason `landed` is a closure at all — answering it costs the paint a
        // geometry read against the live adjustment. A panicking closure is what makes
        // "was it consulted?" observable; asserting only the returned verdict cannot
        // tell a short-circuit from a discarded answer.
        let never = || panic!("the gate consulted the scroll on a branch that decides without it");
        assert_eq!(
            pending_open_gate(false, false, never),
            PendingOpenGate::Idle
        );
        assert_eq!(
            pending_open_gate(true, true, never),
            PendingOpenGate::Abandon
        );
    }

    #[test]
    fn a_heading_reads_the_band_slot_its_level_names() {
        assert_eq!(band_slot(0, 5), 0);
        assert_eq!(band_slot(4, 5), 4);
    }

    #[test]
    fn a_heading_level_past_the_last_slot_folds_onto_it_rather_than_panicking() {
        // The defence this function exists to be: the alternative on the paint path is
        // an index panic inside a `snapshot` callback. `levels == 0` is unreachable
        // from the theme model and is still answered rather than underflowed, because
        // `levels - 1` on a `usize` is the one spelling that would turn this guard into
        // the crash it prevents.
        assert_eq!(band_slot(9, 5), 4);
        assert_eq!(band_slot(9, 0), 0);
    }
}
