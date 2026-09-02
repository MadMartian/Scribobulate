//! The `vadjustment::value-changed` recorder, and the shape of an emission sequence.
//!
//! Split out of [`super::trace`] when a second experiment ([`super::margin`]) needed
//! the same instrument. The cut is by cause rather than by size: this file owns
//! **watching the adjustment being written to and describing the sequence**, and its
//! callers own **the question each sequence is asked**. Two experiments recording the
//! same signal through two copies of this apparatus would be free to drift apart
//! silently, and the whole value of the second experiment is that its numbers are
//! comparable with the first's.
//!
//! # Why the vadjustment, and what it can and cannot see
//!
//! `GtkTextLayout` is not in the gtk-rs bindings, so its `::changed` cannot be hooked
//! from here. `gtktextview.c`'s `changed_handler` (4.6.9) reaches the adjustment
//! through `gtk_adjustment_set_value`, so `value-changed` is a proxy for it — and the
//! proxy is LOSSY IN ONE DIRECTION, which decides how its numbers may be read. The
//! compensating write sits inside `if (new_first_para_top != old_first_para_top)`
//! (around `:4920`), and `GtkAdjustment` swallows a set to a value it already holds.
//! So a `::changed` that computes a ZERO compensation emits nothing:
//!
//! **an emission count is a LOWER BOUND on the number of `::changed` emissions.** A
//! count that tracks a dose is evidence; a count BELOW it is not evidence against, and
//! must not be reported as a refutation.

use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// One `value-changed` emission: how far it moved the adjustment from the value
/// standing at the previous emission (or, for the first, from the value standing when
/// the trace was armed).
///
/// The DELTA and not the value, because that is the shape the compensation shows up
/// in: a run of equal deltas is one reference reused across the run.
#[derive(Clone, Copy)]
pub(super) struct Emission {
    pub(super) delta: f64,
}

/// A recorder for every `vadjustment::value-changed` across one window of the run.
///
/// Armed and disarmed through [`super::harness::Phase`] rather than left connected for
/// the rig's whole life, so the toggle's own settle is what it records and the rig's
/// teardown is not attributed to it.
#[derive(Default)]
pub(super) struct Trace {
    log: Rc<RefCell<Vec<Emission>>>,
    armed: RefCell<Option<(gtk::Adjustment, glib::SignalHandlerId)>>,
}

impl Trace {
    pub(super) fn arm(&self, adjustment: &gtk::Adjustment) {
        let previous = Rc::new(std::cell::Cell::new(adjustment.value()));
        let log = Rc::clone(&self.log);
        // Captures no widget — the handler takes its emitter as an argument, which is
        // the corrective for a closure a GObject's own machinery owns (GTK4Rs/AP-63).
        let id = adjustment.connect_value_changed(move |adjustment| {
            let value = adjustment.value();
            log.borrow_mut().push(Emission {
                delta: value - previous.get(),
            });
            previous.set(value);
        });
        *self.armed.borrow_mut() = Some((adjustment.clone(), id));
    }

    pub(super) fn disarm(&self) {
        if let Some((adjustment, id)) = self.armed.borrow_mut().take() {
            adjustment.disconnect(id);
        }
    }

    pub(super) fn emissions(&self) -> Vec<Emission> {
        self.log.borrow().clone()
    }
}

/// The emission sequence collapsed to runs of consecutive IDENTICAL deltas.
///
/// This is the shape the mechanism turns on, and neither a count nor a sum can express
/// it: a run of equal deltas is one reference reused across the run, and a run of
/// shrinking ones is a reference re-resolved at each step.
pub(super) fn runs(emissions: &[Emission]) -> Vec<(usize, f64)> {
    let mut runs: Vec<(usize, f64)> = Vec::new();
    for e in emissions {
        match runs.last_mut() {
            Some((n, delta)) if *delta == e.delta => *n += 1,
            _ => runs.push((1, e.delta)),
        }
    }
    runs
}

/// The runs rendered as `10x-7.0px, 1x-88.0px, ...` — the whole sequence in one line,
/// which is what makes a per-delta comparison between two arms readable side by side.
pub(super) fn runs_summary(emissions: &[Emission]) -> String {
    let mut out = String::new();
    for (i, (n, delta)) in runs(emissions).iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("{n}x{delta:+.1}px"));
    }
    out
}
