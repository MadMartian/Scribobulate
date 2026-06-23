//! The desktop's light/dark preference — the source GTK does not supply on Windows.
//!
//! **GTK 4.22.4 does not read the Windows theme setting at all.** Verified against the
//! shipped `gtk-4-1.dll`, which contains no reference to `AppsUseLightTheme`,
//! `SystemUsesLightTheme` or `Themes\Personalize` (searched as both ASCII and UTF-16).
//! Without this module the app renders light *everywhere* under a dark desktop —
//! editor, chrome and caption alike — however correct the rest of the theming is.
//!
//! Named to match `platform::mac::appearance` on purpose: same job, different OS
//! mechanism. This is a separate cause from [`super::frame`] even though the caption is
//! one of its consumers — what it produces feeds the editor scheme, the toolbar and the
//! outline sidebar through the one theme key, and the caption is merely the consumer
//! that made the gap visible first.

use gtk::glib;
use std::ffi::c_void;

use super::wide;

#[link(name = "advapi32")]
extern "system" {
    fn RegGetValueW(
        key: isize,
        subkey: *const u16,
        value: *const u16,
        flags: u32,
        kind: *mut u32,
        data: *mut c_void,
        data_size: *mut u32,
    ) -> i32;
}

/// `HKEY_CURRENT_USER` — the theme preference is per-user, not machine-wide.
const HKEY_CURRENT_USER: isize = 0x8000_0001_u32 as i32 as isize;
/// `RRF_RT_REG_DWORD`: refuse to coerce; if the value is not a DWORD, fail rather
/// than reinterpret bytes.
const RRF_RT_REG_DWORD: u32 = 0x0000_0018;

/// How often to re-read the Windows theme preference. See
/// [`track_system_dark_mode`] for why this polls rather than subscribes.
const THEME_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// Whether Windows is set to a dark **app** theme, or `None` if the preference
/// cannot be read.
///
/// Reads `AppsUseLightTheme`, not `SystemUsesLightTheme`: Windows tracks the two
/// separately, and the one users mean by "apps dark, taskbar light" is this one.
/// The value is inverted — 0 means dark.
///
/// `None` is not the same as "light". The key is absent on a freshly-imaged
/// profile that has never opened Personalization, and treating that as an explicit
/// light preference would be a guess; callers leave GTK's own default alone
/// instead.
fn windows_prefers_dark() -> Option<bool> {
    let subkey = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let value = wide("AppsUseLightTheme");
    let mut data: u32 = 0;
    let mut size: u32 = std::mem::size_of::<u32>() as u32;

    // SAFETY: both name buffers are NUL-terminated and outlive the call; `data`
    // and `size` are live and agree, and RRF_RT_REG_DWORD makes the API reject
    // anything that is not a 4-byte DWORD rather than overrun the buffer.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            std::ptr::addr_of_mut!(data).cast::<c_void>(),
            std::ptr::addr_of_mut!(size),
        )
    };
    // ERROR_SUCCESS == 0. Anything else (typically ERROR_FILE_NOT_FOUND) means
    // there is no preference to honour.
    (status == 0).then_some(data == 0)
}

/// Make the app follow the Windows light/dark app-theme setting, now and whenever
/// the user changes it. Call once, from `startup`, **before** the first window is
/// built so it opens in the right theme rather than flashing light and re-rendering.
///
/// **GTK does not do this for us on Windows.** Verified against the shipped GTK
/// 4.22.4: `gtk-4-1.dll` contains no reference to `AppsUseLightTheme`,
/// `SystemUsesLightTheme` or `Themes\Personalize`, so nothing in the toolkit ever
/// consults the setting. Without this the app renders light under Windows dark
/// mode no matter what the user has chosen — and the dark-caption work above would
/// be inert, since `desktop_is_dark()` could never become true.
///
/// This deliberately writes into the GTK theme-variant settings rather than
/// re-theming anything directly, and it writes them through
/// [`crate::colorscheme::apply_desktop_lightness`] rather than by hand. Two
/// separate reasons:
///
/// * `gtk-application-prefer-dark-theme` is already the app's live-theme channel —
///   `app::setup::connect_theme_change` subscribes to it and re-renders every window
///   (and re-applies the captions). So this supplies the missing *source* for a path
///   that already exists, rather than adding a parallel one. It is the same shape as
///   the fix the KDE/X11 live-theme-toggle gap needs — there the channel is a portal
///   signal, here it is the registry, and in both cases the repaint machinery
///   downstream is already built.
/// * That property is **deprecated as of GTK 4.20**, which moved theme-variant
///   selection to `@media (prefers-color-scheme: …)` driven by
///   `gtk-interface-color-scheme` — and GDK-Win32 sets that one no more than Quartz
///   does, so it sits at `UNSUPPORTED` here. On macOS that combination paints the
///   window light however correct every other reading is (ScrAP-184). **Measured on
///   Windows, it does not**: gvsbuild's 4.22.4 with the `Default` theme still paints
///   correctly from the legacy property alone, both directions, so the defect
///   predicted for this platform did not reproduce (QA round R1-04). Both are written
///   regardless — `UNSUPPORTED` is one GTK or theme update away from becoming
///   load-bearing with no signal in between, and the honest guard is
///   [`the_window_actually_paints_the_desktop_lightness`], which samples pixels and
///   so fails loudly if the legacy path ever stops working.
///
/// **Why it polls.** The precise signal is `WM_SETTINGCHANGE`/`ImmersiveColorSet`,
/// but receiving it means owning a window procedure, and GDK owns the toplevel's.
/// `RegNotifyChangeKeyValue` is the other option and needs a thread to wait on —
/// this app runs entirely on the GLib main loop with no worker threads (TECH.md),
/// and a two-second registry read is far cheaper than the first thread in the
/// codebase. The write is change-gated, so a steady state costs one registry read
/// per tick and nothing else — without that gate, every tick would fire the notify
/// and re-render every window twice a second.
pub(crate) fn track_system_dark_mode() {
    let mut last = windows_prefers_dark();
    if let Some(dark) = last {
        crate::colorscheme::apply_desktop_lightness(dark);
    }

    glib::timeout_add_local(THEME_POLL_INTERVAL, move || {
        let now = windows_prefers_dark();
        if now != last {
            last = now;
            if let Some(dark) = now {
                // Writes both theme-variant settings and fires the notify that
                // `connect_theme_change` is listening on.
                crate::colorscheme::apply_desktop_lightness(dark);
            }
        }
        glib::ControlFlow::Continue
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the POLARITY of the theme preference, which is the one thing here that
    /// is easy to get exactly backwards and impossible to notice in review: the
    /// registry value is `AppsUseLightTheme`, so **0 means dark**. Inverting it
    /// would make the app light in dark mode and dark in light mode — still
    /// "working", still passing every other check.
    ///
    /// Reads the raw DWORD independently and asserts our mapping against it, so the
    /// test follows the machine's real setting instead of assuming one.
    #[test]
    fn dark_is_the_inverse_of_apps_use_light_theme() {
        let subkey = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
        let value = wide("AppsUseLightTheme");
        let mut raw: u32 = u32::MAX;
        let mut size: u32 = std::mem::size_of::<u32>() as u32;
        // SAFETY: as in `windows_prefers_dark` — NUL-terminated names, a live
        // 4-byte destination, and a type-restricted read.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                std::ptr::addr_of_mut!(raw).cast::<c_void>(),
                std::ptr::addr_of_mut!(size),
            )
        };

        if status == 0 {
            assert_eq!(
                windows_prefers_dark(),
                Some(raw == 0),
                "AppsUseLightTheme={raw}: 0 must map to dark, non-zero to light",
            );
        } else {
            // No preference recorded — must report absence, NOT a guessed "light".
            assert_eq!(
                windows_prefers_dark(),
                None,
                "an unreadable preference must be None so GTK's own default stands",
            );
        }
    }
}

/// Closes the loop between the registry and what the user actually sees.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;
    use gtk::prelude::*;

    /// Closes the loop between the registry and what the user sees, without
    /// touching the registry: the *detection* half writes
    /// `gtk-application-prefer-dark-theme`, and everything downstream — the theme
    /// sheet, the editor scheme, the caption — keys off `desktop_is_dark()`. If
    /// that property did not move the probe, `track_system_dark_mode` would be
    /// writing into a void and the app would stay light in Windows dark mode with
    /// every other test still green.
    ///
    /// Worth asserting rather than assuming: the property makes Adwaita select its
    /// dark variant, which is a different mechanism from `GTK_THEME=Adwaita:dark`,
    /// and it depends on the shipped theme actually carrying that variant.
    #[gtktest::test]
    fn preferring_dark_moves_the_probe_the_whole_app_themes_from() {
        let Some(settings) = gtk::Settings::default() else {
            panic!("a display-backed test must have GtkSettings");
        };
        let restore = settings.is_gtk_application_prefer_dark_theme();

        for want_dark in [true, false] {
            settings.set_gtk_application_prefer_dark_theme(want_dark);
            assert_eq!(
                crate::palette::desktop_is_dark(),
                want_dark,
                "prefer-dark-theme={want_dark} must be visible to the desktop probe",
            );
        }

        settings.set_gtk_application_prefer_dark_theme(restore);
    }

    /// Pump the main loop until `cond` holds or `budget` iterations elapse.
    /// Non-blocking `iteration(false)`, never `iteration(true)`, so a condition that
    /// never becomes true returns promptly and the caller's assertion fires instead
    /// of the suite hanging (GTK4Rs/AP-79).
    fn pump_until(budget: u32, cond: impl Fn() -> bool) -> bool {
        let ctx = glib::MainContext::default();
        for _ in 0..budget {
            if cond() {
                return true;
            }
            ctx.iteration(false);
            std::thread::sleep(std::time::Duration::from_millis(4));
        }
        cond()
    }

    /// Present a small window, render it through GSK and return its centre pixel —
    /// the app's own painted output, with no screen capture in the loop.
    ///
    /// The Cairo renderer specifically: that is the one `lib.rs` pins for the shipped
    /// binary, so this samples the renderer the product actually uses. Sampling a
    /// texture rather than screenshotting also cannot land on the wrong desktop,
    /// which is a failure that reads exactly like "the app is light".
    fn painted_centre_pixel() -> Option<(u8, u8, u8)> {
        const W: usize = 300;
        const H: usize = 200;

        let window = gtk::Window::new();
        window.set_default_size(W as i32, H as i32);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
        content.set_hexpand(true);
        content.set_vexpand(true);
        content.append(&gtk::Label::new(Some("paint check")));
        window.set_child(Some(&content));
        // A realized-but-unmapped window has no allocation and snapshots to nothing
        // at all, so this must be presented and given the loop a chance to lay it
        // out. That failure is reported as "no render node" rather than as a wrong
        // colour, which is at least honest.
        window.present();
        pump_until(100, || content.width() > 0);

        let paintable = gtk::WidgetPaintable::new(Some(&window));
        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(
            snapshot.upcast_ref::<gtk::gdk::Snapshot>(),
            W as f64,
            H as f64,
        );
        let node = snapshot.to_node();
        let px = node.and_then(|node| {
            let renderer = gtk::gsk::CairoRenderer::new();
            renderer.realize(None::<&gtk::gdk::Surface>).ok()?;
            let texture = renderer.render_texture(&node, None);
            let (w, h) = (texture.width() as usize, texture.height() as usize);
            let mut data = vec![0u8; w * h * 4];
            texture.download(&mut data, w * 4);
            let idx = ((h / 2) * w + w / 2) * 4;
            // Cairo ARGB32 on a little-endian host: B, G, R, A.
            Some((data[idx + 2], data[idx + 1], data[idx]))
        });
        window.destroy();
        px
    }

    /// **The assertion the Windows port was missing.** Every other dark-mode check
    /// here reads intermediate state —
    /// `preferring_dark_moves_the_probe_the_whole_app_themes_from` above asserts the
    /// palette moves — and on macOS intermediate state stayed perfectly correct
    /// through an entire session in which the window painted the wrong colour
    /// (ScrAP-184). macOS closed that by asserting pixels; Windows inherited the same
    /// exposure and not the check (QA round R1-04). This is the check.
    ///
    /// **It is a guard, not a reproduction.** Measured on a real Windows host when it
    /// was written, the defect it guards does NOT occur here: gvsbuild's GTK 4.22.4
    /// with the `Default` theme paints `rgb(53,53,53)`/`rgb(246,245,244)` correctly
    /// from the legacy property alone, even with `gtk-interface-color-scheme` at
    /// `UNSUPPORTED`. That is worth stating rather than leaving for someone to infer
    /// from a passing test: this exists because the legacy property is deprecated and
    /// the platform is one GTK or theme update away from macOS's behaviour, with no
    /// other signal that it happened.
    ///
    /// Goes through `colorscheme::apply_desktop_lightness` — the shared writer both
    /// platforms use — rather than setting the properties itself, so it samples the
    /// code the app runs. Drives BOTH directions, and forces the legacy property to
    /// the wrong value first so the writer's change-gate always fires: a
    /// one-direction assertion would pass on a light-themed machine with the writer
    /// doing nothing at all.
    #[gtktest::test]
    fn the_window_actually_paints_the_desktop_lightness() {
        use gtk::prelude::ObjectExt;

        const PROPERTY: &str = "gtk-interface-color-scheme";
        let Some(settings) = gtk::Settings::default() else {
            panic!("a display-backed test must have GtkSettings");
        };
        // Restore both channels: this body runs in the shared main-thread suite, and
        // GtkSettings is process-global.
        let restore_legacy = settings.is_gtk_application_prefer_dark_theme();
        let restore_scheme = settings
            .find_property(PROPERTY)
            .map(|_| settings.property_value(PROPERTY));

        let mut wrong = Vec::new();
        for want_dark in [true, false] {
            settings.set_gtk_application_prefer_dark_theme(!want_dark);
            assert!(
                crate::colorscheme::apply_desktop_lightness(want_dark),
                "the writer must report a change after the property was forced the other way",
            );
            match painted_centre_pixel() {
                Some((r, g, b)) => {
                    let painted_dark = (u32::from(r) + u32::from(g) + u32::from(b)) / 3 < 110;
                    if painted_dark != want_dark {
                        wrong.push(format!("dark={want_dark} paints rgb({r},{g},{b})"));
                    }
                }
                None => wrong.push(format!(
                    "dark={want_dark} produced no render node at all — cannot judge the paint"
                )),
            }
        }

        settings.set_gtk_application_prefer_dark_theme(restore_legacy);
        if let Some(value) = restore_scheme {
            settings.set_property_from_value(PROPERTY, &value);
        }

        assert!(
            wrong.is_empty(),
            "the window does not paint the lightness it was told to: {}. The settings and \
             the resolved palette can be perfectly correct and this still fail — that is the \
             whole reason this check exists (QA round R1-04).",
            wrong.join("; "),
        );
    }
}
