//! Releasing a window's keyboard focus before the widget holding it is torn down, so the
//! text input method can let go of it while it still can.
//!
//! **Why this exists (Windows, GTK 4.24.1).** GTK's Win32 input-method context adds a
//! message filter to the DISPLAY when its widget gains focus, with the context as the
//! filter's data, and removes it only from its focus-out. Since GTK 4.24 (MR !9958) that
//! focus-out first looks up the widget's surface and returns early when there is none, and
//! the context's dispose no longer removes the filter. A focused editor, entry or text view
//! that is unrealized before it loses focus — its window closing, its tab closing — leaves
//! the filter behind with a freed context as its data, and every later Win32 message on the
//! display passes that freed pointer to the filter. That is a use-after-free in GTK: a
//! `gtk_im_context_ime_message_filter` critical when the freed memory fails the type check
//! (fatal under `G_DEBUG=fatal-criticals`), an access violation or a reused object when it
//! does not. MEASURED on the Windows seat: the same test binary run against GTK 4.24.1 and
//! 4.22.4, 16 of the GTK suite's cases fail on 4.24.1 with that critical and none on 4.22.4.
//!
//! **The fix is upstream and not yet released**: GNOME/gtk commit 2a50bd4af8, MR !10411 (removes
//! the filter on dispose too). **Retire this module** once every platform's shipped GTK
//! carries it — on Windows that is the gvsbuild release pinned in the CI workflow — and the
//! GTK suite on Windows stays free of that critical without it.
//!
//! **What it does.** When a widget is unrealized while it is, or contains, its window's
//! focus widget, the window's focus is cleared first. GTK delivers that focus change
//! synchronously, so the input method's focus-out runs while the widget still has its
//! surface, and removes the filter. GTK would move the focus away anyway — but later, after
//! the surface is gone.
//!
//! **One choke point for every teardown.** Windows close, tabs close and move out, popovers
//! and dialogs with entries go away, in the app's code and in GTK's own. All of them
//! unrealize their widgets, so one emission hook on `GtkWidget::unrealize` reaches every
//! one of them; wiring each destroy site would miss the next one anybody adds. Emission
//! hooks run before the signal's class handler, which is the one that destroys the surface.
//! It is not platform-gated: clearing focus from a widget about to be torn down is harmless
//! on every platform, and running everywhere keeps it exercised by every suite.

use gtk::glib;
use gtk::glib::translate::IntoGlib;
use gtk::prelude::*;
use std::cell::Cell;

thread_local! {
    /// Whether the emission hook is installed. Once per process, never removed.
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
    /// Whether the hook acts. Only a test switches it off, to show it is what matters.
    static ENABLED: Cell<bool> = const { Cell::new(true) };
}

/// Install the `unrealize` emission hook on `GtkWidget`, once. Main thread only, after GTK
/// has initialised, and before the first window is built.
pub(crate) fn install() {
    if INSTALLED.with(|i| i.replace(true)) {
        return;
    }
    let ty = gtk::Widget::static_type();
    // `g_signal_lookup` finds a signal only once its class is initialised. The class
    // reference is deliberately never dropped: the hook lives as long as the process.
    std::mem::forget(glib::Class::<glib::Object>::from_type(ty));
    let signal = glib::subclass::signal::SignalId::lookup("unrealize", ty)
        .expect("GtkWidget has an `unrealize` signal");
    // SAFETY: `on_unrealize_hook` matches `GSignalEmissionHook`, the signal id is valid for
    // GtkWidget, and no user data is passed, so there is nothing to destroy.
    unsafe {
        glib::gobject_ffi::g_signal_add_emission_hook(
            signal.into_glib(),
            0,
            Some(on_unrealize_hook),
            std::ptr::null_mut(),
            None,
        );
    }
}

/// The `GSignalEmissionHook` for `GtkWidget::unrealize`. Returns `TRUE` to stay installed.
unsafe extern "C" fn on_unrealize_hook(
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
        if let Ok(widget) = instance.get::<gtk::Widget>() {
            release_focus_before_unrealize(&widget);
        }
    });
    glib::ffi::GTRUE
}

/// Clear the focus of `widget`'s window when the focus widget is `widget` or inside it.
fn release_focus_before_unrealize(widget: &gtk::Widget) {
    if !ENABLED.with(Cell::get) {
        return;
    }
    let Some(window) = widget.root().and_downcast::<gtk::Window>() else {
        return;
    };
    let Some(focus) = gtk::prelude::RootExt::focus(&window) else {
        return;
    };
    if &focus == widget || focus.is_ancestor(widget) {
        GtkWindowExt::set_focus(&window, None::<&gtk::Widget>);
    }
}

#[cfg(all(test, feature = "gtk-integration-tests"))]
mod tests {
    use super::*;
    use crate::testpump::{until, Clock};
    use std::rc::Rc;

    /// The hook switched off until dropped, so a failed assertion cannot leave every later
    /// case in this suite process unprotected.
    struct Disabled;

    impl Disabled {
        fn new() -> Self {
            ENABLED.with(|e| e.set(false));
            Self
        }
    }

    impl Drop for Disabled {
        fn drop(&mut self) {
            ENABLED.with(|e| e.set(true));
        }
    }

    /// A presented window with an editable text view inside a box, and the editor focused.
    struct Host {
        window: gtk::Window,
        holder: gtk::Box,
        editor: gtk::TextView,
        /// At the editor's focus-out: whether it still had a surface, which is the one
        /// thing GTK 4.24's Win32 input-method focus-out needs to remove its filter.
        /// `None` until it loses focus.
        left_with_surface: Rc<Cell<Option<bool>>>,
    }

    /// Build a [`Host`], or `None` (announced) when the editor's focus never reached it as
    /// an input-method focus would — a window that never became active. The controller is
    /// added BEFORE the focus moves: one added later never sees the focus arrive, so it
    /// never reports it leaving.
    fn host(case: &str) -> Option<Host> {
        install();
        let window = gtk::Window::new();
        let holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let editor = gtk::TextView::new();
        let left_with_surface = Rc::new(Cell::new(None));
        let focus = gtk::EventControllerFocus::new();
        focus.connect_leave({
            let left_with_surface = left_with_surface.clone();
            move |c| {
                let surface = c
                    .widget()
                    .and_then(|w| w.native())
                    .and_then(|n| n.surface());
                if left_with_surface.get().is_none() {
                    left_with_surface.set(Some(surface.is_some()));
                }
            }
        });
        editor.add_controller(focus.clone());
        holder.append(&editor);
        window.set_child(Some(&holder));
        window.set_default_size(300, 200);
        window.present();
        until(Clock::Frame, "the window to map", || window.is_mapped());
        editor.grab_focus();
        until(Clock::Frame, "the editor to take focus", || {
            editor.has_focus()
        });
        if !focus.is_focus() {
            crate::testsymlink::skipped(
                case,
                "the window never became active, so the editor's focus never reached it \
                 as an input method's would and there is no focus-out to observe",
            );
            window.destroy();
            return None;
        }
        Some(Host {
            window,
            holder,
            editor,
            left_with_surface,
        })
    }

    /// **A focused editor removed from a live window loses its focus while it still has a
    /// surface** — the tab-close shape. GTK's own unparent only SCHEDULES the focus move,
    /// and runs it after the editor has left the window, when there is no surface to find.
    /// The control is the same removal with the hook off, which must lose the focus without
    /// a surface (or never), or this test is not about the hook.
    #[gtktest::test]
    fn a_focused_editor_removed_from_its_window_loses_focus_while_it_has_a_surface() {
        const CASE: &str = "focus release: editor removed from a live window";
        let Some(control) = host(CASE) else { return };
        {
            let _off = Disabled::new();
            control.holder.remove(&control.editor);
        }
        until(Clock::Frame, "the scheduled focus move", || {
            gtk::prelude::RootExt::focus(&control.window).is_none()
        });
        assert_ne!(
            control.left_with_surface.get(),
            Some(true),
            "precondition: with the hook off the editor kept its surface through its \
             focus-out, so this case cannot tell the hook from GTK's own teardown"
        );
        control.window.destroy();

        let Some(case) = host(CASE) else { return };
        case.holder.remove(&case.editor);
        assert_eq!(
            case.left_with_surface.get(),
            Some(true),
            "the editor lost focus without a surface (or not at all) as it left the window: \
             GTK 4.24's Win32 input method then leaves its message filter on a freed context"
        );
        case.window.destroy();
    }
    // No window-close case: an active window that hides already gives its focus widget a
    // focus-out with its surface, so such a case passes with the hook removed (measured)
    // and would guard nothing. The removal case above is the one GTK gets wrong.
}
