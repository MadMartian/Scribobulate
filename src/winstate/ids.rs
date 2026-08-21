//! The stable tab- and window-identity newtypes.

use gtk::prelude::*;
use gtk::ApplicationWindow;

/// A window's stable identity — a newtype over `window.as_ptr() as u64`, the raw
/// GObject pointer of a live window reused as a per-window key (the window
/// outlives every use, and the GLib main loop is single-threaded, so pointer
/// reuse is not a hazard — see the `registry` module doc). Distinct from
/// [`TabId`] so the two can never be transposed by the type system (QA round-1
/// L2 newtyped the tab side and deliberately left the window side raw; this
/// completes that decision).
///
/// CONTRACT: a window has exactly ONE identity, and it is derived ONLY through
/// [`WindowId::of`]. That identity is used in two places that MUST agree — the
/// [`registry`](super::registry)'s `WINDOWS` HashMap key, and the window's
/// `.scrib-win-<id>` CSS-scope class ([`window`](crate::window)'s per-window
/// zoom-provider scope, GTK4Rs/AP-77). If those two ever derived the id independently
/// and diverged, a cross-window tab move would resolve state against one window
/// and CSS against another (a latent desync). Flowing both from `WindowId::of`
/// makes that divergence unrepresentable.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct WindowId(u64);

impl WindowId {
    /// Derive a window's identity from its live GObject pointer. The single
    /// source for both the registry key and the `.scrib-win-<id>` CSS scope.
    pub(crate) fn of(window: &ApplicationWindow) -> WindowId {
        WindowId(window.as_ptr() as u64)
    }

    /// The raw identity value — for the registry HashMap key and the
    /// `.scrib-win-{}` CSS-class string only. Prefer passing `WindowId` directly.
    pub(crate) fn raw(self) -> u64 {
        self.0
    }
}

/// A tab's stable identity — a newtype over the incrementing counter, distinct
/// from a window's raw `u64` id so the two can never be transposed by the type
/// system (QA round-1 L2). Allocated by [`alloc_tab_id`](super::alloc_tab_id). The
/// inner value is only exposed at FFI/serialization boundaries (the GLib drag
/// payload and the `select-tab` GAction string target) via [`TabId::raw`] /
/// [`TabId::from_raw`] / `Display` / `FromStr`; everywhere else pass the `TabId` itself.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct TabId(u64);

impl TabId {
    /// The raw counter value — for the GLib drag `GValue` and the `select-tab`
    /// action target string only. Prefer passing `TabId` directly elsewhere.
    pub(crate) fn raw(self) -> u64 {
        self.0
    }

    /// Wrap a raw value read back from a boundary (drag `GValue`, action target).
    pub(crate) fn from_raw(raw: u64) -> Self {
        TabId(raw)
    }
}

impl std::fmt::Display for TabId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for TabId {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<u64>().map(TabId)
    }
}

/// `WindowId::of` reads a live `ApplicationWindow` pointer, so it needs an
/// initialized GTK — hence a `#[gtktest::test]` behind `gtk-integration-tests`
/// (GTK4Rs/AP-71: a plain `#[test]` + `gtk::init()` panics on the second test).
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    /// The identity is STABLE for a given window (same pointer every call — the
    /// property both the registry key and the CSS scope rely on), and its `.raw()`
    /// round-trips through the `.scrib-win-<id>` CSS-class suffix format both
    /// call sites format it into.
    #[gtktest::test]
    fn window_id_is_stable_and_matches_the_css_suffix() {
        let app = gtk::Application::new(
            Some("com.extollit.scribobulate.windowidtest"),
            gtk::gio::ApplicationFlags::NON_UNIQUE,
        );
        // `register` is what emits `GApplication::startup`, and an `ApplicationWindow`
        // built before that raises "New application windows must be added after the
        // GApplication::startup signal has been emitted" — a `Gtk-CRITICAL` that this
        // suite emitted for some time while still reporting `ok`, because nothing in
        // the toolchain ran the `G_DEBUG=fatal-criticals` POLICY prescribes. It is not
        // cosmetic: under that flag the process dies with `SIGTRAP`, so the gate could
        // never be armed while this line stood.
        app.register(gtk::gio::Cancellable::NONE)
            .expect("registering a NON_UNIQUE application cannot contact a bus or fail");
        let window = ApplicationWindow::new(&app);

        let a = WindowId::of(&window);
        let b = WindowId::of(&window);
        assert_eq!(a, b, "the same window must yield the same WindowId");

        // The exact string window/mod.rs and zoom.rs build; its suffix must parse
        // back to the raw id, proving registry key and CSS scope share one value.
        let css_class = format!("scrib-win-{}", a.raw());
        let suffix = css_class.strip_prefix("scrib-win-").unwrap();
        assert_eq!(
            suffix.parse::<u64>().unwrap(),
            a.raw(),
            "the CSS-class suffix must be exactly WindowId::raw()"
        );
    }
}
