//! Test scaffolding shared across `window`'s test modules.
//!
//! Exists because the same eight lines were written four times under four names
//! (`test_app` in `mod.rs`, `outline_nav.rs` and `save.rs`; `make_app` in
//! `navhistory::testkit`) plus a long tail of inline copies. Each copy registers a
//! `gtk::Application` before a window is built, and each one that drifts drifts silently:
//! a missing `register` does not fail loudly, it produces a window whose actions are
//! wired to an application that never joined the action map.

#[cfg(all(test, feature = "gtk-integration-tests"))]
use gtk::prelude::*;

/// A registered, non-unique `gtk::Application` for a test that needs to build a window.
///
/// `NON_UNIQUE` because several of these exist in one process and must not contend for a
/// bus name; `register` before returning because `ApplicationWindow::new` on an
/// unregistered application yields a window whose `app.*` actions resolve to nothing.
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) fn test_app(id: &str) -> gtk::Application {
    let app = gtk::Application::new(Some(id), gtk::gio::ApplicationFlags::NON_UNIQUE);
    app.register(gtk::gio::Cancellable::NONE)
        .expect("register before building a window");
    app
}

/// As [`test_app`], applying this suite's application-id convention to `suffix`.
///
/// The convention was previously spelled out at one call site and implied at the others,
/// which is how two of them ended up with ids that shared a prefix by coincidence rather
/// than by rule.
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) fn test_app_suffixed(suffix: &str) -> gtk::Application {
    test_app(&format!(
        "com.extollit.scribobulate.integrationtest.{suffix}"
    ))
}

/// Resize a MAPPED window to `width`×`height`, as one step of a frame-edge drag does.
///
/// On X11 `set_default_size` is enough, because a mapped toplevel follows it. On
/// GDK-Win32 it is not: the window stayed 900 px wide through ten calls asking for 940
/// down to 400. Measured on Windows, GTK 4.24.1, the native frame. So on Windows the
/// native window is resized as well, which is what a real drag does, and GDK follows it.
/// The remembered size is set on every platform. Otherwise a later GDK-Win32 layout pass
/// that falls back to it (GTK4Rs/AP-158, see `platform::win32::frame`) would snap the
/// window back.
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) fn resize_mapped_window(window: &impl IsA<gtk::Window>, width: i32, height: i32) {
    window.set_default_size(width, height);
    #[cfg(windows)]
    crate::platform::win32::frame::set_client_size(window.as_ref(), width, height);
}
