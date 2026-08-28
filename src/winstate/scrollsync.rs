//! State for the coalesced, frame-clock-driven split editor↔preview scroll sync.

use std::cell::{Cell, RefCell};

/// Which split pane is the scroll-sync *source*; the other pane mirrors it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ScrollDriver {
    Editor,
    Preview,
}

/// State for the coalesced, frame-clock-driven split editor↔preview scroll sync,
/// modeled on GtkSourceView's `GtkSourceMap` (the canonical "synced second view").
///
/// GtkTextView validates line heights over many idle passes after `set_buffer`,
/// during which the preview adjustment's `upper` thrashes and it emits a storm of
/// `notify::upper` (every pass) and `value-changed` (on clamp). Mirroring scroll
/// synchronously on each emission mirrors half-validated garbage and oscillates
/// (GTK4Rs/AP-16). Instead every notification is *coalesced* into a single tick callback
/// that re-projects `driver → follower` once per frame (on the frame-clock UPDATE
/// phase, which runs BEFORE this frame's layout/allocate — ANTI-PATTERNS
/// deferred-work meta-pattern correction). Correctness comes from the projection
/// being idempotent and re-convergent — it re-runs each frame until the thrash
/// converges — NOT from any single post-layout read.
pub(crate) struct ScrollSync {
    /// The source pane the other mirrors. Forced to `Editor` for the duration of a
    /// programmatic preview re-render, so the preview's validation noise can never
    /// drive the editor; genuine user input on the preview switches it to `Preview`.
    pub(crate) driver: Cell<ScrollDriver>,
    /// At most one outstanding tick callback — all adjustment notifications
    /// coalesce into it so projection runs once per frame; being idempotent it
    /// re-converges across frames, so it need not (and does not) run after this
    /// frame's validation.
    pub(crate) tick: RefCell<Option<gtk::TickCallbackId>>,
    /// One-stack-frame reentrancy guard wrapping the synchronous follower
    /// `set_value`, so the resulting `value-changed` does not bounce back.
    pub(crate) guard: Cell<bool>,
    /// Last (value, upper) seen on the editor / preview adjustments — drops the
    /// redundant notifications GtkAdjustment emits during validation.
    pub(crate) ed_last: Cell<(f64, f64)>,
    pub(crate) pv_last: Cell<(f64, f64)>,
    /// The reading position last WRITTEN into a pane by a view-mode hand-off, with
    /// the pane line it was written to.
    ///
    /// A hand-off that always re-derives the position from the destination's
    /// geometry cannot be idempotent, because a restore does not land the requested
    /// line at exactly the requested pixel: the next capture reads a viewport top a
    /// little above it, maps that to the preceding waypoint, and the pair ratchets
    /// one block per trip — measured walking a settled 40-section fixture 79 → 62 →
    /// 58 → 54, upward, without bound. Remembering what was written closes the loop:
    /// if the pane has not moved since (its top line is still the line written),
    /// nothing has happened that the stored position does not already describe, so
    /// it is handed back unchanged and a round trip is exact. A user scroll changes
    /// the top line and the stored value is discarded.
    pub(crate) applied_reading: Cell<Option<(crate::readingpos::DocPosition, i32)>>,
}

impl Default for ScrollSync {
    fn default() -> Self {
        Self {
            driver: Cell::new(ScrollDriver::Editor),
            tick: RefCell::new(None),
            guard: Cell::new(false),
            ed_last: Cell::new((-1.0, -1.0)),
            pv_last: Cell::new((-1.0, -1.0)),
            applied_reading: Cell::new(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::winstate::*;

    #[test]
    fn scroll_sync_defaults_to_editor_driver() {
        let s = ScrollSync::default();
        assert_eq!(s.driver.get(), ScrollDriver::Editor);
        assert!(!s.guard.get());
        assert!(s.tick.borrow().is_none());
    }
}
