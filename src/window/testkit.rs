//! Test scaffolding shared across `window`'s test modules.
//!
//! Exists because the same eight lines were written four times under four names
//! (`test_app` in `mod.rs`, `outline_nav.rs` and `save.rs`; `make_app` in
//! `navhistory::testkit`) plus a long tail of inline copies. Each copy registers a
//! `gtk::Application` before a window is built, and each one that drifts drifts silently:
//! a missing `register` does not fail loudly, it produces a window whose actions are
//! wired to an application that never joined the action map.

#[cfg(test)]
use gtk::prelude::*;

/// A registered, non-unique `gtk::Application` for a test that needs to build a window.
///
/// `NON_UNIQUE` because several of these exist in one process and must not contend for a
/// bus name; `register` before returning because `ApplicationWindow::new` on an
/// unregistered application yields a window whose `app.*` actions resolve to nothing.
#[cfg(test)]
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
#[cfg(test)]
pub(crate) fn test_app_suffixed(suffix: &str) -> gtk::Application {
    test_app(&format!(
        "com.extollit.scribobulate.integrationtest.{suffix}"
    ))
}
