//! "Something is happening" for a document operation that is taking a while.
//!
//! # Why this exists, and why it did not before
//!
//! While document I/O ran on the main thread, a slow filesystem froze the window.
//! That was bad, but it was *unmistakable*: the application visibly stopped. Moving
//! the I/O off the main thread fixed the freeze and, on its own, replaced it with
//! something arguably worse as an affordance — a window that stays perfectly live and
//! says **nothing at all** for as long as the filesystem takes. Measured against the
//! slow-filesystem rig: a save took roughly thirteen seconds with no spinner, no busy
//! cursor and no status text, the only acknowledgement being the "File saved." toast
//! long afterwards. To the person who pressed Save, that reads as *"my keystroke did
//! nothing"*, and the natural response is to press it again.
//!
//! So the responsiveness fix owes a progress indication. This is it.
//!
//! # The delay is the whole design
//!
//! A notice raised the instant an operation starts would flicker on every save on a
//! local disk, where the whole thing completes in under a millisecond — and a control
//! that flickers is noise the eye learns to discard, which is worse than no control.
//! So the notice is **armed, not shown**: a timer runs for [`BUSY_NOTICE_DELAY`], and
//! only if the operation is still outstanding when it fires does anything appear.
//! Fast operations therefore show nothing, ever, and slow ones announce themselves at
//! the point where a person starts to wonder.
//!
//! # Releasing is not the caller's job to remember
//!
//! The notice retracts when the guard is dropped, on every path — success, I/O error,
//! an early return, an unwind. A raw push/pop pair would put a retraction obligation
//! on every exit from an operation that already has several, and the failure mode of
//! forgetting one is an un-retractable message sitting in the footer **permanently**,
//! with no error and no log line (the Status-notice CAM's whole subject). Same
//! reasoning, and the same shape, as [`WriteGate`](super::WriteGate).
//!
//! It is `Rc`-backed so one notice can span a *logical* operation made of several
//! futures — the save guard's read, the decision, and the write are three, and the
//! user experiences them as one "Saving…". The notice lifts when the last clone
//! drops.

use super::{StatusCtx, WindowChrome};
use std::cell::Cell;
use std::rc::{Rc, Weak};
use std::time::Duration;

/// How long an operation must be outstanding before it admits to it.
///
/// 500 ms is the threshold below which a person reads a delay as "instant" and above
/// which they start looking for feedback. Low enough that a genuinely slow filesystem
/// is reported almost immediately; high enough that no local-disk operation ever
/// reaches it, so the ordinary case stays visually silent.
pub(crate) const BUSY_NOTICE_DELAY: Duration = Duration::from_millis(500);

/// A pending "this is taking a moment" notice. Retracts on drop.
#[derive(Clone)]
pub(crate) struct BusyNotice {
    // Never read, and that is the point: this handle exists only to OWN the inner
    // state, whose `Drop` does the retraction. Reading it would mean the retraction
    // had become somebody's job to call.
    #[allow(dead_code, reason = "RAII handle: held for its Drop, never inspected")]
    inner: Rc<BusyNoticeInner>,
}

#[cfg(test)]
impl BusyNotice {
    /// Whether the notice has actually reached the status bar yet — i.e. whether the
    /// operation outlived [`BUSY_NOTICE_DELAY`]. Test-only: production code never asks,
    /// because arming and dropping is the whole protocol.
    pub(crate) fn is_showing(&self) -> bool {
        let shown = self.inner.shown.take();
        self.inner.shown.set(shown);
        shown.is_some()
    }
}

struct BusyNoticeInner {
    /// Weak, and captured at arm time — the notice must retract against the stack that
    /// *issued* it. Resolving the chrome afresh at retraction time would pop this
    /// window's handle out of a different window's stack after a cross-window tab move,
    /// match nothing, and strand the message here forever (Status-notice CAM column C).
    chrome: Weak<WindowChrome>,
    /// `Some` once the delay has elapsed and the notice is actually on screen.
    shown: Cell<Option<StatusCtx>>,
    /// The arming timer, cancelled if the operation finishes first.
    timer: Cell<Option<gtk::glib::SourceId>>,
}

impl BusyNotice {
    /// Arm a notice on `chrome`'s status bar, to appear only if this guard is still
    /// alive [`BUSY_NOTICE_DELAY`] from now.
    pub(crate) fn arm(chrome: &Rc<WindowChrome>, message: &'static str) -> Self {
        let inner = Rc::new(BusyNoticeInner {
            chrome: Rc::downgrade(chrome),
            shown: Cell::new(None),
            timer: Cell::new(None),
        });
        // Weak, so an armed notice cannot keep its own operation's window alive; the
        // timer is cancelled on drop anyway, and this is the belt to that braces.
        let weak = Rc::downgrade(&inner);
        let id = gtk::glib::timeout_add_local_once(BUSY_NOTICE_DELAY, move || {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            inner.timer.set(None);
            if let Some(chrome) = inner.chrome.upgrade() {
                inner
                    .shown
                    .set(Some(chrome.status.borrow_mut().push(message)));
            }
        });
        inner.timer.set(Some(id));
        Self { inner }
    }
}

impl Drop for BusyNoticeInner {
    fn drop(&mut self) {
        if let Some(id) = self.timer.take() {
            // Finished before the delay elapsed: nothing was ever shown, and nothing
            // should be. This is the common case on a local disk.
            id.remove();
        }
        if let Some(ctx) = self.shown.take() {
            if let Some(chrome) = self.chrome.upgrade() {
                chrome.status.borrow_mut().pop(ctx);
            }
        }
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod tests {
    use super::*;
    use crate::winstate::chrome;
    use gtk::prelude::*;

    /// A window of its own per test. The id must be UNIQUE per test: both bodies run
    /// in one process on one GTK thread, and a second `register` of a live id fails.
    fn window(id: &str) -> gtk::ApplicationWindow {
        let app = gtk::Application::new(
            Some(&format!(
                "com.extollit.scribobulate.integrationtest.busy{id}"
            )),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        app.register(gtk::gio::Cancellable::NONE).expect("register");
        crate::window::new_window(&app, "IT", "body", None)
    }

    /// **A fast operation shows nothing, ever.** This is the half that keeps the
    /// control meaningful: every save on a local disk finishes in well under the
    /// delay, and a footer that blinked on each one would be noise the eye learns to
    /// ignore — at which point the notice stops working for the slow case too.
    #[gtktest::test]
    fn an_operation_that_finishes_quickly_never_shows_a_notice() {
        let win = window("fast");
        let ch = chrome(&win).expect("chrome");
        {
            let notice = BusyNotice::arm(&ch, "Saving…");
            assert!(!notice.is_showing(), "nothing appears immediately");
        }
        // Pump well past the delay: the guard is gone, so the armed timer must have
        // been cancelled rather than firing into an empty stack.
        assert!(!crate::docio::settle(|| ch
            .status
            .borrow()
            .label_text()
            .contains("Saving")));
        assert!(
            !ch.status.borrow().label_text().contains("Saving"),
            "a cancelled notice must never reach the label"
        );
        win.destroy();
    }

    /// **An operation that outlives the delay announces itself, and retracts on drop.**
    #[gtktest::test]
    fn a_slow_operation_shows_a_notice_and_retracts_it() {
        let win = window("slow");
        let ch = chrome(&win).expect("chrome");
        let notice = BusyNotice::arm(&ch, "Saving…");
        assert!(
            crate::docio::settle(|| ch.status.borrow().label_text().contains("Saving")),
            "an operation still outstanding after {BUSY_NOTICE_DELAY:?} must say so — \
             a live, silent window reads as a keystroke that did nothing"
        );
        assert!(notice.is_showing());

        drop(notice);
        assert!(
            !ch.status.borrow().label_text().contains("Saving"),
            "and must retract the instant the operation ends, without waiting for \
             anything else to happen"
        );
        win.destroy();
    }
}
