//! macOS only. Puts the application's menus where this desktop keeps them — the
//! **system** menu bar at the top of the screen — instead of inside the window.
//!
//! ## What the platform lacks, and what this supplies
//!
//! Nothing here is application behaviour: the menu model is the same one
//! `app::build_menubar` assembles for every platform, and GTK already knows how
//! to render a `GMenuModel` as a native `NSMenu`. What is missing is the
//! *plumbing* — GTK's Quartz backend takes exactly one application-wide menubar
//! model, while this app's menus carry per-window content (`View ▸ Documents`
//! lists that window's tabs; `Format ▸`'s Link/Image items relabel Insert↔Edit
//! for that window's selection), so something has to keep the exported model
//! pointed at whichever window is active. That is this module's whole job.
//!
//! ## Why not calling `set_menubar` was the bug
//!
//! `app::setup` deliberately never called `gtk_application_set_menubar`, on the
//! reasoning that doing so would *export the model to a global-menu shell and
//! duplicate the in-window bar* (GTK4Rs/AP-76). That reasoning was drawn from
//! Linux's opt-in global-menu shells, and on Quartz it is exactly inverted:
//! `gtk_application_impl_quartz_startup` installs a native `NSMenu`
//! **unconditionally**, and when `gtk_application_get_menubar` returns NULL it
//! substitutes a built-in fallback — the `default-menu` resource in
//! `gtk/ui/gtkapplication-quartz.ui`, an app menu plus a stub `Edit` and
//! `Window` (GTK 4.22.4, `gtkapplication-quartz.c:320-333`). MEASURED before the
//! fix: the app's native bar read `scribobulate | Edit | Window`, that fallback
//! verbatim, while the real File/Edit/Format/View/Help bar sat inside the window.
//! So *withholding* the model is what produced two menu bars; supplying it
//! replaces the stub with the real thing and leaves exactly one.
//!
//! The in-window bar is not hidden here — it is never built (`BuiltMenubar::bar`
//! is `None` on this platform), so there is no second surface to keep in step.
//!
//! ## Why the export is re-pointed on every activation
//!
//! `gtk_application_impl_quartz_set_menubar` replaces one section of a `GMenu`
//! the native menu's item tracker observes, so a later `set_menubar` updates the
//! live `NSMenu` rather than needing it rebuilt. Action *state* needs no help at
//! all: `gtk_application_impl_quartz_active_window_changed` re-points its muxer's
//! `win` group at the newly active window (`gtkapplication-quartz.c:369-395`), so
//! `win.*` items in the exported model resolve against, and grey out for, the
//! window the user is actually looking at. Only model *content* had to be
//! followed, which is what this does.

use gtk::prelude::*;

/// Keep the system menu bar showing the active window's own menu model.
///
/// Call once, from `startup`, before any window exists — it subscribes to
/// `notify::active-window` and exports on each change, so it needs no list of
/// windows and no teardown. A window whose chrome is not registered yet (or an
/// activation with no window at all, as when the last one closes) leaves the
/// current menu in place rather than blanking the bar.
pub(crate) fn track_active_window(app: &gtk::Application) {
    app.connect_active_window_notify(|app| {
        let Some(window) = app.active_window() else {
            return;
        };
        let Some(window) = window.downcast_ref::<gtk::ApplicationWindow>() else {
            return;
        };
        let Some(chrome) = crate::winstate::chrome(window) else {
            return;
        };
        app.set_menubar(Some(&chrome.menu_model));
    });
}
