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

/// One `value-changed` emission: the value GTK published, and how far that moved the
/// adjustment from the value standing at the previous emission (or, for the first,
/// from the value standing when the trace was armed).
#[derive(Clone, Copy)]
pub(super) struct Emission {
    pub(super) value: f64,
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
                value,
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

/// What the adjustment was told to move by, in total, across every logged emission.
pub(super) fn sum_of_deltas(emissions: &[Emission]) -> f64 {
    emissions.iter().map(|e| e.delta).sum()
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

/// The longest run of consecutive identical deltas — [`runs`]'s finding as one number,
/// so "uniform" can be asserted as a shape rather than as a literal sequence a host's
/// font metrics would break.
pub(super) fn longest_uniform_run(emissions: &[Emission]) -> usize {
    runs(emissions).iter().map(|(n, _)| *n).max().unwrap_or(0)
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

/// How one emission sequence's deltas sit against another's, compared one by one.
///
/// The shape an experiment that varies something about the VIEW rather than the
/// document needs: the question is not what the deltas are but how far each moved when
/// the knob did, and only a per-delta comparison can tell "every write is short by the
/// same amount" from "the total happens to differ".
pub(super) enum SequenceShift {
    /// The two sequences are different lengths, so there is no elementwise comparison
    /// to make. Reported rather than worked around — whether the count moves with the
    /// knob is itself part of what a knob experiment is asking.
    NotComparable { ours: usize, theirs: usize },
    /// Same length, and the deltas did not all move by the same amount.
    NotUniform { n: usize, lo: f64, hi: f64 },
    /// Same length, and every delta moved by exactly `shift`.
    Uniform { n: usize, shift: f64 },
}

/// Compare `ours` against `control` delta by delta. Both must be non-empty; a caller
/// with nothing recorded has a rig problem rather than a comparison to make.
pub(super) fn shift_against(ours: &[Emission], control: &[Emission]) -> SequenceShift {
    if ours.len() != control.len() {
        return SequenceShift::NotComparable {
            ours: ours.len(),
            theirs: control.len(),
        };
    }
    let shifts: Vec<f64> = ours
        .iter()
        .zip(control)
        .map(|(ours, theirs)| ours.delta - theirs.delta)
        .collect();
    let lo = shifts.iter().cloned().fold(f64::INFINITY, f64::min);
    let hi = shifts.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let n = shifts.len();
    if lo == hi {
        SequenceShift::Uniform { n, shift: lo }
    } else {
        SequenceShift::NotUniform { n, lo, hi }
    }
}
