//! Writing the desktop's light/dark preference **into** GTK — the counterpart to
//! `palette::desktop_is_dark()`, which reads the resolved answer back out.
//!
//! **Nothing on Linux calls this**, which is why the module is `#[cfg]`-gated at its
//! declaration in `lib.rs`, exactly as `workaround` is: there the desktop writes
//! `gtk-application-prefer-dark-theme` itself and GTK reads it. This
//! exists for the two platforms where GTK supplies no *source* for that channel and
//! a thin platform module has to — `platform/win32/appearance.rs` (the registry) and
//! `platform/mac/appearance.rs` (a CoreFoundation distributed notification). Both
//! call in here rather than writing GtkSettings themselves.
//!
//! **Why it is shared rather than one copy per platform.** What follows is a
//! *GTK-version* rule, not a platform one: which of the two settings actually
//! selects the dark theme depends on the GTK and the theme in use, not on the OS.
//! It lived inside the macOS module until QA round R1-04, so Windows wrote only the
//! legacy half — not because Windows had decided to, but because that was the only
//! half visible from there. One definition, two callers, neither platform owning it.
//!
//! ⚠ **The two platforms measure DIFFERENTLY on this exact rule, so do not collapse
//! them into one claim.** Both run GTK 4.22.4:
//!
//! * **macOS** (Homebrew) needs the modern property. Writing only the legacy one
//!   leaves the window painted light while every intermediate reading stays correct
//!   — ScrAP-184, and the reason `tests/macos_dark_mode.rs` samples pixels.
//! * **Windows** (gvsbuild) does **not**, as of this writing. Measured on a real
//!   host: the theme resolves to `Default`, `gtk-interface-color-scheme` sits at
//!   `UNSUPPORTED`, and the legacy property alone still paints
//!   `rgb(53,53,53)`/`rgb(246,245,244)` correctly in both directions — so the
//!   failure predicted for it did not reproduce.
//!
//! Windows writes both anyway, and that is a deliberate call rather than
//! cargo-culting: the legacy property is deprecated as of 4.20 and the modern one is
//! `UNSUPPORTED` rather than absent, so the platform is one GTK or theme update away
//! from the macOS behaviour, with no signal in between. The guard that makes this
//! safe to assume is not the write but the **pixel** test beside each caller — if the
//! legacy path ever stops working, they fail loudly instead of quietly painting light.
//!
//! Why the two builds differ at one version, and why neither platform's result may be
//! inferred from the other's: **ScrAP-205**.
//!
//! Deliberately a **leaf**: it references nothing else in the crate. That is what
//! lets `tests/macos_dark_mode.rs` pull it in by `#[path]` alongside the module it
//! tests, so the pixel assertion there runs the very code the app runs.

use gtk::glib;

/// `GtkInterfaceColorScheme` (`gtk/gtkenums.h`, Since: 4.20). The discriminants are
/// public ABI; they are written out here because the typed binding for them sits
/// behind gtk4-rs's `v4_20` feature, and enabling that would raise the whole
/// project's floor to GTK 4.20 — the Linux build targets 4.6.9.
const COLOR_SCHEME_DARK: i32 = 2;
const COLOR_SCHEME_LIGHT: i32 = 3;

/// The 4.20+ setting. Named as a constant because it is looked up by string on
/// purpose — see [`set_interface_color_scheme`].
const INTERFACE_COLOR_SCHEME: &str = "gtk-interface-color-scheme";

/// Write the desktop's lightness into the settings the theme machinery reads, and
/// report whether anything changed. Nothing here touches a widget.
///
/// **Both properties, deliberately** — they are not redundant, and which one does
/// the work depends on the GTK version:
///
/// * `gtk-application-prefer-dark-theme` is the legacy channel (Deprecated: 4.20).
///   It still works: it drives `gtk_css_provider_load_named(theme, "dark")`. It is
///   also the property `app::setup::connect_theme_change` subscribes to, so it is
///   what triggers the app's own re-render — dropping it would leave the theme
///   sheet, editor scheme and per-tab previews un-refreshed even if the toolkit
///   restyled itself.
/// * `gtk-interface-color-scheme` is the modern channel (Since: 4.20). GTK 4.20+
///   made the default theme's `gtk.css` a set of `@media (prefers-color-scheme: …)`
///   imports driven by this setting, and **`UNSUPPORTED`/`DEFAULT`/`LIGHT` all
///   evaluate as light** — only `DARK` selects dark. Neither the Quartz backend nor
///   GDK-Win32 sets it (each carries DPI, fonts and click timings and nothing about
///   appearance), so on both platforms it stays `UNSUPPORTED` unless an app supplies
///   it. Windows ships GTK 4.22, i.e. past the threshold.
///
/// **Writing only the legacy one reads correct everywhere except the screen.** It
/// moves `theme_base_color`, so `palette::desktop_is_dark()` follows, the palette
/// resolves dark and every re-render runs with `dark=true` — while the window still
/// paints light. That is why the tests that guard this assert *pixels*
/// (`platform/win32/appearance.rs`'s `the_window_actually_paints_the_desktop_lightness`,
/// `tests/macos_dark_mode.rs` § 4) and not the settings.
///
/// Change-gated on the legacy property: writing it unconditionally would fire
/// `notify` and re-render every window and tab even when nothing changed. The gate
/// is safe for the colour scheme too, because this is the only writer of either —
/// they move together or not at all, and the one state it can skip from is a fresh
/// process, where `UNSUPPORTED` and a light preference already agree.
pub(crate) fn apply_desktop_lightness(dark: bool) -> bool {
    let Some(settings) = gtk::Settings::default() else {
        return false;
    };
    if settings.is_gtk_application_prefer_dark_theme() == dark {
        return false;
    }
    log::debug!(
        "desktop appearance: switching to {}",
        if dark { "dark" } else { "light" }
    );

    set_interface_color_scheme(&settings, dark);
    // Last, so the notify that drives the app's re-render is emitted once the colour
    // scheme above is already in place — the re-render re-resolves the palette from
    // the live theme, and doing it the other way round would build the theme sheet
    // from the outgoing one.
    settings.set_gtk_application_prefer_dark_theme(dark);
    true
}

/// The 4.20+ half of [`apply_desktop_lightness`]. A no-op, without complaint, on any
/// GTK that does not carry the property.
///
/// Set dynamically rather than through the typed setter, and guarded on the property
/// actually existing, so a build against a pre-4.20 GTK silently skips it instead of
/// emitting a `g_object_set` critical for an unknown property. The Linux reference
/// platform is GTK 4.6.9, so that branch is live, not theoretical.
pub(crate) fn set_interface_color_scheme(settings: &gtk::Settings, dark: bool) {
    use gtk::glib::translate::ToGlibPtrMut;
    use gtk::prelude::ObjectExt;

    let Some(pspec) = settings.find_property(INTERFACE_COLOR_SCHEME) else {
        log::debug!(
            "desktop appearance: {INTERFACE_COLOR_SCHEME} absent (GTK < 4.20); legacy channel only"
        );
        return;
    };

    let mut value = glib::Value::from_type(pspec.value_type());
    // SAFETY: `value` was just initialised to the property's own type, which is the
    // `GtkInterfaceColorScheme` enum, and the discriminant written is one of that
    // enum's declared members.
    unsafe {
        glib::gobject_ffi::g_value_set_enum(
            value.to_glib_none_mut().0,
            if dark {
                COLOR_SCHEME_DARK
            } else {
                COLOR_SCHEME_LIGHT
            },
        );
    }
    settings.set_property_from_value(INTERFACE_COLOR_SCHEME, &value);
}
