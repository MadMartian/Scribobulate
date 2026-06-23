//! The single transient info toast shared by every one-shot "that happened" notice.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;

/// The floating, auto-dismissing info notice: a bottom-right icon+label box shown
/// briefly after a clean reload (TDD 5.4) or a successful save, then hidden again.
///
/// **One widget serves every transient notice**, retargeted per message, rather than
/// one box per message type. That is not merely DRY: every toast is anchored to the
/// same bottom-right corner of the same overlay, so two visible at once would paint
/// on top of each other. A single reused widget makes them mutually exclusive *by
/// construction* — a save right after a reload retargets the notice in place instead
/// of stacking a second box over the first. (The conflict toast is deliberately NOT
/// one of these: it is a persistent, button-bearing prompt that waits for an answer,
/// not a notice that fades.)
///
/// A cloneable *handle*, like the GTK widgets it wraps: every field is refcounted
/// (widgets internally, the timer via `Rc`), so a clone shares the same toast and the
/// same pending dismissal rather than duplicating either. That lets it be cloned into
/// `WindowChrome` alongside the plain `gtk::Box` fields it sits next to.
#[derive(Clone)]
pub(crate) struct InfoToast {
    /// The floating box mounted into the window's content overlay. Its visibility is
    /// toggled, never re-created.
    widget: gtk::Box,
    icon: gtk::Image,
    label: gtk::Label,
    /// The pending auto-dismiss timeout. Shared with the timeout closure itself (a
    /// plain `Rc<RefCell<..>>`, no widget in it, so there is no reference cycle to
    /// leak) so the closure can clear the id as it fires — see [`show`](Self::show).
    timer: Rc<RefCell<Option<glib::SourceId>>>,
}

impl InfoToast {
    pub(crate) fn new(widget: gtk::Box, icon: gtk::Image, label: gtk::Label) -> Self {
        Self {
            widget,
            icon,
            label,
            timer: Rc::new(RefCell::new(None)),
        }
    }

    /// The floating box, for mounting into the content overlay.
    pub(crate) fn widget(&self) -> &gtk::Box {
        &self.widget
    }

    /// Retarget the notice to `icon_name`/`text`, show it, and (re)arm its
    /// auto-dismiss for `for_`.
    ///
    /// A second notice while one is still up cancels the in-flight timer and
    /// reschedules, so the toast stays visible for the full delay from the *latest*
    /// notice rather than expiring on the first one's clock (TDD 5.4).
    pub(crate) fn show(&self, icon_name: &str, text: &str, for_: Duration) {
        // Cancel any in-flight auto-dismiss before rescheduling.
        if let Some(id) = self.timer.borrow_mut().take() {
            id.remove();
        }
        self.icon.set_icon_name(Some(icon_name));
        self.label.set_text(text);
        self.widget.set_visible(true);

        let widget = self.widget.downgrade();
        let timer = Rc::clone(&self.timer);
        let id = glib::timeout_add_local_once(for_, move || {
            if let Some(w) = widget.upgrade() {
                w.set_visible(false);
            }
            // Drop the now-dispatched id. Load-bearing: a `SourceId` is invalid once
            // its source has fired, and calling `remove()` on a stale one is a GLib
            // error — so the next `show()` must find `None` here, not a spent id.
            timer.replace(None);
        });
        self.timer.replace(Some(id));
    }

    /// Hide the notice now, cancelling any pending auto-dismiss (e.g. a tab switch,
    /// where a notice about the outgoing tab must not linger over the incoming one).
    pub(crate) fn hide(&self) {
        if let Some(id) = self.timer.borrow_mut().take() {
            id.remove();
        }
        self.widget.set_visible(false);
    }
}
