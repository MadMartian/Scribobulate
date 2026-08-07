//! The atomic-save round-trip guard.

use std::cell::Cell;

/// The atomic-save round-trip guard's arm/consume/clear protocol (GTK4Rs/AP-62),
/// extracted as a plain, GTK-free data type (QA round-2
/// R2-2) — its three methods are the ONLY way to touch the underlying flag,
/// enforcing at the type level what used to be a convention spread across
/// five call sites in two modules (`window/save.rs`, `app.rs`) with no
/// static link between them (the original GTK4Rs/AP-1 structural root cause). Being
/// plain data with no GTK dependency, it is unit-tested directly here rather
/// than only reachable through a live, `gtk::init()`-requiring `TabState`.
#[derive(Default)]
pub(crate) struct SelfDeleteGuard(Cell<bool>);

impl SelfDeleteGuard {
    /// Arm the guard immediately before the tab's own `write_atomic` rename.
    pub(crate) fn arm(&self) {
        self.0.set(true);
    }

    /// Disarm WITHOUT treating it as a consumed self-delete — for when the
    /// write that would have armed it never actually happened (a failed
    /// `write_atomic`), when a monitor is freshly (re)pointed and can have
    /// no legitimate pending self-`Deleted` to swallow (the M1 fix), or when
    /// a `Changed`/`Created` event proves no separate `Deleted` is coming.
    pub(crate) fn disarm(&self) {
        self.0.set(false);
    }

    /// Consume the guard: `true` means a `Deleted` event just observed was
    /// our own self-triggered rename (swallow it); `false` means it's a
    /// genuine external deletion the caller should surface. Always leaves
    /// the guard disarmed afterward, so it can only ever swallow one event
    /// per arm.
    pub(crate) fn consume(&self) -> bool {
        self.0.replace(false)
    }
}

#[cfg(test)]
mod tests {
    use crate::winstate::*;

    #[test]
    fn self_delete_guard_starts_disarmed() {
        let g = SelfDeleteGuard::default();
        assert!(!g.consume());
    }

    #[test]
    fn self_delete_guard_arm_then_consume_returns_true_once() {
        let g = SelfDeleteGuard::default();
        g.arm();
        assert!(g.consume());
        // Consuming disarms — a second consume without re-arming is a
        // genuine external event, not a swallowed self-write.
        assert!(!g.consume());
    }

    #[test]
    fn self_delete_guard_disarm_after_arm_prevents_consume() {
        let g = SelfDeleteGuard::default();
        g.arm();
        g.disarm();
        assert!(!g.consume());
    }
}
