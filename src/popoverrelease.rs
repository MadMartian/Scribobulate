//! Releasing a popover's surface after it closes, so a hidden popover cannot keep the
//! frame clock running.
//!
//! **Why this exists (macOS).** GDK's Quartz backend at GTK 4.22.4 keeps painting empty
//! frames, once per display refresh and forever, after ANY `GtkPopover` has been shown
//! and hidden. The app does nothing in those frames (no tick callback, no `queue_draw`,
//! no snapshot), yet it holds ~4.6% of a core and starts a fresh CVDisplayLink thread
//! every frame (GTK4Rs/AP-359). A popover that is hidden and then UNREALIZED leaves
//! nothing ticking. The macOS seat measured both, in a bare gtk4-rs window and in this
//! app. Linux and Windows idle at 0% after the same popup and popdown, so only macOS
//! turns this on. MEASURED with this module, macOS 4.22.4: 4.35% to 0% idle after a
//! dropdown and a context menu. It is switched on from `app::setup` ([`set_enabled`]).
//! The mechanism is shared code, so every platform's suite tests it.
//!
//! **One choke point for every popover.** The app builds popovers in many places, and
//! GTK builds more itself: a `GtkDropDown`'s list, the context menu of a `GtkText`, every
//! `GtkMenuButton`'s popover. All of them are `GtkPopover`s and all of them emit
//! `closed`, so one emission hook on that signal reaches every one of them. Wiring each
//! site instead would miss GTK's own and the next one anybody adds.
//!
//! **Why the release waits.** `closed` fires from the hide vfunc (GTK4Rs/AP-192), and a
//! popover that mapped under the pointer has armed GTK's tooltip hover timeout against
//! itself. Unrealizing it before that timeout fires leaves the timeout aimed at a dead
//! surface (GTK4Rs/AP-117), which is a `GDK_IS_SURFACE` critical, and fatal under
//! `G_DEBUG=fatal-criticals`. So the release runs [`TOOLTIP_HOVER_DRAIN`] after the LAST
//! close, and only if the popover is still hidden then. A reopen cancels nothing — it
//! emits no `closed` — so the pending release finds the popover visible and leaves it;
//! a second close replaces the pending release, so it cannot fire early.
//!
//! The next `popup()` realizes the popover again. Whether that brings back the
//! first-popup validation the preview's marker pre-warm exists to absorb
//! (GTK4Rs/AP-118) is checked by `tests/MANUAL-TEST.md` 1.8a and 17.35 (a).

use gtk::glib;
use gtk::glib::translate::IntoGlib;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};

/// How long after a popover closes before its surface is released — defined beside the
/// widgets because the toolbar menu's own release waits it too, on every platform.
pub(crate) use crate::widgets::TOOLTIP_HOVER_DRAIN;

thread_local! {
    /// Whether a closed popover is released. Off unless the platform asks for it.
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    /// Whether the emission hook is installed. It is installed once per process and
    /// never removed; [`ENABLED`] is what turns its effect on and off.
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
    /// One pending release per closed popover. A popover that closes again before its
    /// release runs replaces its own entry, so the release always runs a full
    /// [`TOOLTIP_HOVER_DRAIN`] after the LAST close.
    static PENDING: RefCell<Vec<(glib::WeakRef<gtk::Popover>, glib::SourceId)>> =
        const { RefCell::new(Vec::new()) };
}

/// Turn releasing closed popovers on or off, installing the hook the first time it is
/// turned on. Main thread only, like every GTK call.
pub(crate) fn set_enabled(on: bool) {
    if on {
        install();
    }
    ENABLED.with(|e| e.set(on));
}

/// Install the `closed` emission hook on `GtkPopover`, once.
fn install() {
    if INSTALLED.with(|i| i.replace(true)) {
        return;
    }
    let ty = gtk::Popover::static_type();
    // `g_signal_lookup` finds a signal only once its class is initialised. The class
    // reference is deliberately never dropped: the hook lives as long as the process.
    std::mem::forget(glib::Class::<glib::Object>::from_type(ty));
    let signal = glib::subclass::signal::SignalId::lookup("closed", ty)
        .expect("GtkPopover has a `closed` signal");
    // SAFETY: `on_closed_hook` matches `GSignalEmissionHook`, the signal id is valid for
    // GtkPopover, and no user data is passed, so there is nothing to destroy.
    unsafe {
        glib::gobject_ffi::g_signal_add_emission_hook(
            signal.into_glib(),
            0,
            Some(on_closed_hook),
            std::ptr::null_mut(),
            None,
        );
    }
}

/// The `GSignalEmissionHook` for `GtkPopover::closed`. Returns `TRUE` to stay installed.
unsafe extern "C" fn on_closed_hook(
    _ihint: *mut glib::gobject_ffi::GSignalInvocationHint,
    n_param_values: u32,
    param_values: *const glib::gobject_ffi::GValue,
    _data: glib::ffi::gpointer,
) -> glib::ffi::gboolean {
    if n_param_values == 0 || param_values.is_null() {
        return glib::ffi::GTRUE;
    }
    // A panic must not unwind into GLib's C frames.
    let _ = std::panic::catch_unwind(|| {
        // SAFETY: `param_values[0]` is the emitting instance, and `glib::Value` is a
        // transparent wrapper over `GValue`. It is borrowed for this call only.
        let instance = unsafe { &*(param_values as *const glib::Value) };
        if let Ok(popover) = instance.get::<gtk::Popover>() {
            schedule_release(&popover);
        }
    });
    glib::ffi::GTRUE
}

/// Schedule `popover`'s release [`TOOLTIP_HOVER_DRAIN`] from now, replacing any release
/// already pending for it.
fn schedule_release(popover: &gtk::Popover) {
    if !ENABLED.with(Cell::get) {
        return;
    }
    let superseded = take_pending(popover);
    if let Some(id) = superseded {
        // Unfired: a release that fires removes its own entry before it does anything
        // else, so an id still in `PENDING` has not run (GTK4Rs/AP-128, GTK4Rs/AP-353).
        id.remove();
    }
    let weak = popover.downgrade();
    let id = glib::timeout_add_local_once(TOOLTIP_HOVER_DRAIN, {
        let weak = weak.clone();
        move || {
            let popover = weak.upgrade();
            PENDING.with(|p| {
                p.borrow_mut()
                    .retain(|(w, _)| w.upgrade().is_some_and(|q| Some(&q) != popover.as_ref()))
            });
            if let Some(popover) = popover {
                release_if_hidden(&popover);
            }
        }
    });
    PENDING.with(|p| p.borrow_mut().push((weak, id)));
}

/// Remove and return the pending release for `popover`, dropping any entry whose popover
/// is gone.
fn take_pending(popover: &gtk::Popover) -> Option<glib::SourceId> {
    PENDING.with(|p| {
        let mut p = p.borrow_mut();
        let mut found = None;
        let mut kept = Vec::with_capacity(p.len());
        for (weak, id) in p.drain(..) {
            match weak.upgrade() {
                Some(q) if &q == popover => found = Some(id),
                Some(_) => kept.push((weak, id)),
                // Its popover is gone, so its release would only upgrade to nothing.
                None => id.remove(),
            }
        }
        *p = kept;
        found
    })
}

/// Unrealize `popover` if it is still hidden and still realized. A popover reopened since
/// it closed keeps its surface, and one already torn down has nothing to release.
fn release_if_hidden(popover: &gtk::Popover) {
    if !popover.is_visible() && popover.is_realized() {
        WidgetExt::unrealize(popover);
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod tests {
    use super::*;
    use crate::testpump::{drain_for, until, Clock};
    use std::time::Duration;

    /// A presented window with a label to parent popovers to.
    fn host() -> (gtk::Window, gtk::Label) {
        let window = gtk::Window::new();
        let label = gtk::Label::new(Some("host"));
        window.set_child(Some(&label));
        window.set_default_size(300, 200);
        window.present();
        until(Clock::Frame, "the host window to map", || {
            window.is_mapped()
        });
        (window, label)
    }

    fn open(popover: &gtk::Popover) {
        popover.popup();
        until(Clock::Frame, "the popover to map", || popover.is_mapped());
    }

    /// Releasing switched on until dropped, so a failed assertion cannot leave every
    /// later popover in this suite process released behind its back.
    struct Releasing;

    impl Releasing {
        fn on() -> Self {
            set_enabled(true);
            Self
        }
    }

    impl Drop for Releasing {
        fn drop(&mut self) {
            set_enabled(false);
        }
    }

    /// Past the release delay, with slack for the timeout to dispatch.
    fn past_the_delay() {
        drain_for(
            Clock::Frame,
            TOOLTIP_HOVER_DRAIN + Duration::from_millis(300),
        );
    }

    /// **A closed popover gives its surface back, and only once it has stayed closed.**
    ///
    /// The oracle is `is_realized()`, which is the state the macOS frame-clock defect
    /// depends on: a hidden popover that is still realized keeps GDK painting, and one
    /// that is unrealized does not. The control is the same popover with releasing
    /// turned off, which must stay realized, or the assertion is not about this module.
    #[gtktest::test]
    fn a_closed_popover_is_unrealized_after_the_tooltip_delay_and_reopens() {
        let (window, label) = host();
        let popover = gtk::Popover::new();
        // Something focusable: GTK 4.6.9 raises a critical when an autohide popover with
        // nothing to focus opens in a window with no focus widget (`gtk_popover_focus`).
        popover.set_child(Some(&gtk::Button::with_label("body")));
        popover.set_parent(&label);

        // Control: off, a closed popover keeps its surface.
        set_enabled(false);
        open(&popover);
        popover.popdown();
        past_the_delay();
        assert!(
            popover.is_realized(),
            "precondition: with releasing off a closed popover stays realized, else this \
             test cannot tell the release from GTK's own teardown"
        );

        let releasing = Releasing::on();
        open(&popover);
        popover.popdown();
        assert!(
            popover.is_realized(),
            "the release must wait out the tooltip delay"
        );
        past_the_delay();
        assert!(
            !popover.is_realized(),
            "a closed popover still holds its surface — on macOS that keeps the frame \
             clock painting empty frames at ~4.6% of a core"
        );

        // It opens again, realized afresh by `popup()`.
        open(&popover);
        assert!(popover.is_realized() && popover.is_visible());

        // A reopen within the delay: the pending release finds it visible.
        popover.popdown();
        drain_for(Clock::Frame, TOOLTIP_HOVER_DRAIN / 3);
        open(&popover);
        past_the_delay();
        assert!(
            popover.is_mapped(),
            "a release scheduled before a reopen tore down the open popover"
        );

        drop(releasing);
        popover.popdown();
        popover.unparent();
        window.destroy();
    }

    /// **A second close restarts the wait: the release runs a full delay after the LAST
    /// close.** Close, reopen half-way through the delay, close again: the first close's
    /// release is still pending and would find the popover hidden, unrealizing it well
    /// inside the second close's tooltip drain (GTK4Rs/AP-117). Only the supersession in
    /// [`schedule_release`] stops that, and the reopen alone cannot test it — a reopen
    /// emits no `closed`, so `release_if_hidden`'s visibility check is what saves it.
    #[gtktest::test]
    fn a_second_close_restarts_the_release_delay() {
        let (window, label) = host();
        let popover = gtk::Popover::new();
        popover.set_child(Some(&gtk::Button::with_label("body")));
        popover.set_parent(&label);
        let releasing = Releasing::on();

        open(&popover);
        popover.popdown();
        let first_close = std::time::Instant::now();
        drain_for(Clock::Frame, TOOLTIP_HOVER_DRAIN / 2);
        open(&popover);
        popover.popdown();
        let second_close = std::time::Instant::now();
        assert!(
            second_close - first_close < TOOLTIP_HOVER_DRAIN * 4 / 5,
            "precondition: the second close must land while the first close's release is \
             still pending, else this test cannot see it fire early (took {:?})",
            second_close - first_close
        );
        // Past the first close's deadline, inside the second's.
        drain_for(Clock::Frame, TOOLTIP_HOVER_DRAIN * 7 / 10);
        assert!(
            first_close.elapsed() > TOOLTIP_HOVER_DRAIN + Duration::from_millis(100),
            "precondition: the first close's release has had its turn"
        );
        assert!(
            popover.is_realized(),
            "released inside the second close's tooltip drain: the first close's release \
             was not superseded"
        );
        past_the_delay();
        assert!(!popover.is_realized(), "the superseding release never ran");

        drop(releasing);
        popover.unparent();
        window.destroy();
    }

    /// **The hook reaches popovers the app never wires: a subclass and a GTK-built one.**
    /// A `GtkPopoverMenu` is a subclass, and a `GtkMenuButton` builds its popover itself;
    /// a per-site hook would miss the second.
    #[gtktest::test]
    fn a_popover_menu_and_a_menu_buttons_popover_are_released_too() {
        let (window, _label) = host();
        let menu = gtk::gio::Menu::new();
        menu.append(Some("Item"), Some("app.nothing"));
        let button = gtk::MenuButton::new();
        button.set_menu_model(Some(&menu));
        window.set_child(Some(&button));
        until(Clock::Frame, "the button to map", || button.is_mapped());

        let releasing = Releasing::on();
        button.popup();
        let popover = button
            .popover()
            .expect("a menu button with a model has a popover");
        assert!(
            popover.is::<gtk::PopoverMenu>(),
            "precondition: a GTK-built subclass"
        );
        until(Clock::Frame, "the menu to map", || popover.is_mapped());
        button.popdown();
        past_the_delay();
        assert!(
            !popover.is_realized(),
            "the GtkMenuButton's own PopoverMenu kept its surface after closing"
        );

        drop(releasing);
        window.destroy();
    }
}
