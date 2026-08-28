//! **Firing a programmatic navigation's armed open-request** — the paint's own
//! completion event.
//!
//! A step of the paint rather than something bolted after it, because what it
//! consults is the hit-box table [`super::chips`] clears and repopulates on this same
//! pass: run it earlier and it answers from the previous frame.
//! `decorplan::PAINT_ORDER` is where that constraint is written down.
//!
//! **This is a state machine, not a painter, and it is decomposed on its own terms.**
//! The other steps of the paint are "measure a rect, put a colour in it"; this one
//! reads a request armed frames ago, decides between three outcomes, and clears the
//! request as a side effect. Its hard part was never the GTK — it was the PRECEDENCE
//! between expiry, the scroll having landed, and the chip having painted, each of
//! which was learned from a separate defect and none of which any test could reach
//! while the whole thing lived inside a draw callback. That precedence is now
//! [`crate::decorplan::pending_open_gate`], unit-tested headlessly, and what remains
//! here is the two things that genuinely need a live view: asking the adjustment
//! whether the scroll landed, and asking this frame's hit-boxes whether the chip
//! painted.

use super::paint::PaintCtx;
use crate::decorplan::{pending_open_gate, PendingOpenGate};
use gtk::glib;

/// Dispatch the armed request if this paint is the one it was waiting for.
///
/// The request is generated here rather than waited on, because GTK offers no signal
/// for "that paint happened": `GtkTextView` has none, the `GtkTextLayout` that does
/// emit `changed`/`invalidated` is not public GTK4 API, and
/// `GDK_FRAME_CLOCK_PHASE_AFTER_PAINT` is documented "should not be handled by
/// applications". We own the paint, so the completion event is ours to raise.
pub(super) fn fire(ctx: &PaintCtx) {
    let dispatch = {
        let mut pending = ctx.imp.pending_marker_open.borrow_mut();
        // The two live readings the gate arbitrates between. `landed` is deferred into
        // a closure so an expired request never pays for the geometry read — the gate
        // owns which of the two decides, and owning it means owning whether the second
        // is taken at all.
        let verdict = match pending.as_ref() {
            None => PendingOpenGate::Idle,
            Some(p) => pending_open_gate(true, p.deadline.expired(), || {
                // Tested directly against the same aim the converge loop computes, and
                // deliberately NOT against a "has the loop converged?" flag: convergence
                // is observable only through further frame-clock ticks, and under a
                // non-blocking pump the clock can go idle after a single one, leaving a
                // gate that never opens and a request that never dispatches (ScrAP-202,
                // which also records the half-fix this replaced — gating on convergence
                // silenced the very paint the dispatch rides on, because the final
                // `set_value` of a converged loop writes the value already held and so
                // queues no draw). This test needs no extra frame: it is true or false
                // about the state already in front of us, and while it is false the
                // request simply stays armed and the loop keeps aiming. The clamp inside
                // it matters too — where the target cannot reach the top of the viewport
                // the correct landing IS the end of the document, so the test stays
                // satisfiable rather than deadlocking on an unreachable goal.
                ctx.imp.scroll_has_landed_on(p.anchor)
            }),
        };
        match verdict {
            PendingOpenGate::Idle => None,
            PendingOpenGate::Abandon => {
                *pending = None;
                None
            }
            PendingOpenGate::Consult => {
                // The hit-box is consulted as PROOF THAT THE CHIP PAINTED — the
                // completion event this request has been waiting for — and for nothing
                // else. Its RECTANGLE is deliberately not carried forward: a widget rect
                // is only meaningful at the scroll offset it was read at, and handing one
                // across the idle below is precisely how the card came to be positioned
                // where the chip *had been* before the converge-scroll. The card
                // re-derives its own anchor when it is presented.
                let target = pending.as_ref().map(|p| (p.target, p.focus));
                let hit = target.and_then(|(target, focus)| {
                    ctx.imp
                        .marker_hitboxes
                        .borrow()
                        .iter()
                        .find(|(_, idxs)| idxs.contains(&target))
                        // The focus intent travels with the request, not with the paint:
                        // the gesture that decided it returned frames ago.
                        .map(|(_, idxs)| (idxs.clone(), focus))
                });
                if hit.is_some() {
                    *pending = None; // satisfied
                }
                hit
            }
        }
    };
    let Some((idxs, focus)) = dispatch else {
        return;
    };
    // Dispatch on an IDLE, NEVER inline: we are inside the draw path, and
    // `open_marker_popover` calls `popup()`, which re-enters layout/validation
    // (GTK4Rs/AP-22 — forcing layout from snapshot leaves the view stuck blank) and
    // rebuilds widgets mid-emission (GTK4Rs/AP-30 — "broken accounting of active
    // state"). The idle runs after this frame is on screen.
    //
    // WEAK capture, not a strong clone (ScrAP-152/GTK4Rs/AP-128/GTK4Rs/AP-63): a strong
    // clone would pin this view alive as an unrooted zombie if its window is destroyed
    // between this paint and the idle firing, and the idle would then drive
    // `open_marker_popover` → `popup()` a popover on an unrealized view (NULL parent
    // surface → GDK_IS_SURFACE SIGSEGV). De-pinned here, and `open_marker_popover`
    // self-guards on `is_realized()` — defense-in-depth, either alone prevents the
    // crash.
    //
    // The idle also happens to be the POST-PAINT read this path requires. We are inside
    // the paint of the frame whose `size_allocate` already ran `flush_first_validate`,
    // so by the time the idle runs the geometry the card is about to read is validated.
    // That matters because the programmatic navigation is the one case where a pre-paint
    // read is stably WRONG rather than merely late: a tick callback fires at the UPDATE
    // phase, before LAYOUT, so it samples the same pre-validation estimate every frame
    // and a stability check converges on it (GTK4Rs/AP-142). Reading after the paint is
    // the fix, and dispatching from inside the paint is how we get it for free.
    let obj = ctx.view.clone();
    glib::idle_add_local_once(glib::clone!(
        #[weak(rename_to = view)]
        obj,
        move || view.open_marker_popover(&idxs, focus)
    ));
}
