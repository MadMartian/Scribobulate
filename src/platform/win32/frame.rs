//! Native-frame repairs: the two things GTK stopped doing once the OS drew the frame.
//!
//! The app takes the **native Win32 frame** on Windows (`GTK_CSD=0`, POLICY
//! § Architecture rules), which buys real resize borders, the Alt+Space system menu and
//! Snap Layouts — but two things GTK used to do for itself become the OS's job, and GTK
//! does not step back into either:
//!
//! 1. **The caption bar belongs to DWM**, which paints it light unless the window
//!    explicitly asks for the dark one. GTK4 has no Windows theme and never makes that
//!    request. Under CSD this could not arise, because GTK painted the bar itself.
//!
//!    Beware the framing this arrived in — "a light title bar above a dark app" — which
//!    is the *smaller* half and describes a state the build could not produce: GTK
//!    4.22.4 does not read the Windows light/dark setting **at all**, so the app
//!    rendered light *everywhere* under a dark desktop, chrome and editor and caption
//!    alike. A caption-only fix would therefore have been inert. Supplying the missing
//!    source is [`super::appearance`]'s job, and it is a separate cause: it feeds the
//!    editor scheme and the sidebar too, not just this.
//! 2. **The maximize button belongs to the OS**, so maximizing no longer goes through
//!    `gtk_window_maximize()` — and GDK-Win32's toplevel sizing has a hole on exactly
//!    that path, which made a maximized window collapse to its pre-maximize size the
//!    moment any popover opened. See [`track_maximized_size`] for the full derivation.
//!
//! Both are consequences of the *same* decision (native frame instead of CSD), which is
//! why they are one module. Note the second needs no OS call at all and is pure gtk-rs:
//! the organising principle is the **cause**, not the mechanism, so it belongs here
//! anyway.

use gtk::glib;
use gtk::glib::translate::ToGlibPtr;
use gtk::prelude::*;
use std::ffi::c_void;

/// `DWMWA_USE_IMMERSIVE_DARK_MODE`. **20 is the only value needed here**: the
/// 19-vs-20 split exists for pre-20H1 Windows 10, which this port does not
/// target. On a build too old to know the attribute, `DwmSetWindowAttribute`
/// returns `E_INVALIDARG` and the caption simply stays light — the failure mode
/// is the status quo ante, not a crash, which is why there is no version probe.
const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;

// SetWindowPos flags: change nothing about the geometry, just make the frame
// recalculate.
const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOZORDER: u32 = 0x0004;
const SWP_NOACTIVATE: u32 = 0x0010;
const SWP_FRAMECHANGED: u32 = 0x0020;

#[link(name = "gtk-4")]
extern "C" {
    /// Documented GDK-Win32 accessor. Returns the native `HWND` for a **realized**
    /// surface; calling it before the surface exists is what the realize gate in
    /// [`sync_caption_theme`] prevents.
    fn gdk_win32_surface_get_handle(surface: *mut gtk::gdk::ffi::GdkSurface) -> *mut c_void;
}

#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(
        hwnd: *mut c_void,
        attribute: u32,
        value: *const c_void,
        value_size: u32,
    ) -> i32;
}

#[link(name = "user32")]
extern "system" {
    fn SetWindowPos(
        hwnd: *mut c_void,
        insert_after: *mut c_void,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        flags: u32,
    ) -> i32;
}

/// Ask DWM to paint `window`'s caption to match the desktop's lightness.
///
/// Reads [`crate::palette::desktop_is_dark`] rather than taking a `dark` flag, so
/// the caption follows the **same** probe as the editor scheme, the toolbar and
/// the outline sidebar — the one-theme-key rule (POLICY). In particular it must
/// NOT follow the preview's reading theme: selecting Sepia makes the *page* light
/// on a dark desktop, and the caption is chrome, not page (TDD 18.7).
///
/// Safe to call on an unrealized window — it no-ops. Call it again on every
/// desktop theme flip: **DWM remembers the last value set for that `HWND` and
/// does not follow GtkSettings**, so nothing re-applies this for us.
pub(crate) fn sync_caption_theme(window: &impl IsA<gtk::Window>) {
    apply_caption_dark(window, crate::palette::desktop_is_dark());
}

/// The FFI half of [`sync_caption_theme`], split out from the probe so the
/// integration test can drive **both** states on demand. Testing only the ambient
/// one would prove nothing on a light-themed machine: the assertion would hold
/// with `DwmSetWindowAttribute` never called at all, since DWM's default is
/// already light.
fn apply_caption_dark(window: &impl IsA<gtk::Window>, dark: bool) {
    // Before realize there is no native surface and so no HWND. This is why the
    // call is wired to `realize` and not made at builder time.
    let Some(surface) = window.as_ref().surface() else {
        return;
    };

    // SAFETY: `surface` is a live, realized GdkSurface owned by the window we hold
    // a reference to, so the pointer is valid for the duration of the call. On a
    // GTK build without the Win32 backend the surface would not be a
    // GdkWin32Surface — unreachable here, since this module only compiles on
    // Windows, where that is the only backend GTK offers.
    let hwnd = unsafe { gdk_win32_surface_get_handle(surface.to_glib_none().0) };
    if hwnd.is_null() {
        return;
    }

    // BOOL is a 32-bit int, and DWM validates the size — passing a Rust `bool`
    // (1 byte) makes the call fail with E_INVALIDARG.
    let value: i32 = i32::from(dark);
    // SAFETY: `value` outlives the call, and `value_size` matches its real size.
    let hr = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            std::ptr::addr_of!(value).cast::<c_void>(),
            std::mem::size_of::<i32>() as u32,
        )
    };
    if hr != 0 {
        // Not an error worth bothering the user with: the caption keeps whatever
        // it had, which is exactly the behaviour before this module existed.
        log::debug!("DwmSetWindowAttribute(dark={dark}) failed: HRESULT 0x{hr:08x}");
        return;
    }

    // DWM applies the new value to the *next* non-client paint, so on a live theme
    // flip the caption would otherwise stay stale until the window is next
    // activated. A zero-delta SetWindowPos with SWP_FRAMECHANGED forces that paint
    // now without moving, resizing, restacking or focus-stealing the window.
    // SAFETY: `hwnd` is non-null and valid per the check above; a null
    // `insert_after` is ignored because SWP_NOZORDER is set.
    unsafe {
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOSIZE | SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

/// Wire `window` so its caption tracks the desktop theme for the window's whole
/// life: once when the native surface first exists, and again on every flip
/// thereafter (driven from `app::setup::re_render_all_windows`, the single
/// desktop-theme-change hook).
///
/// Connecting to `realize` rather than `map` is deliberate — realize is where the
/// `HWND` first exists, and it runs before the window is shown, so the window
/// never flashes a light caption on a dark desktop.
pub(crate) fn track_caption_theme(window: &gtk::ApplicationWindow) {
    window.connect_realize(sync_caption_theme);
}

/// The size GTK should be told to *remember* for a window whose surface has just
/// been laid out at `actual`, or `None` to leave the remembered size alone.
///
/// Pure, so the rule is unit-testable without a display; [`track_maximized_size`]
/// is the (untestable) signal wiring around it. The rule: **while the window is
/// maximized, and only then, GTK's remembered size must equal the size the OS
/// actually gave it.** Unmaximized, GTK maintains that itself and must be left to
/// — overwriting it there would destroy the size a later restore returns to.
fn remembered_size_while_maximized(
    maximized: bool,
    remembered: (i32, i32),
    actual: (i32, i32),
) -> Option<(i32, i32)> {
    let (width, height) = actual;
    // A zero/negative allocation is a surface that has not been sized yet; writing
    // it would be worse than leaving the remembered size stale.
    if !maximized || width <= 0 || height <= 0 || remembered == actual {
        return None;
    }
    Some(actual)
}

/// Keep GTK's remembered window size in step with the OS while `window` is
/// maximized, so that opening a popover cannot collapse a maximized window back to
/// its pre-maximize size. **GTK4Rs/AP-158.**
///
/// **The symptom.** Maximize the window with the native maximize button, then open
/// *any* popover — a menubar menu, the toolbar's theme dropdown, a right-click
/// context menu — and the window snaps down to whatever size it had before it was
/// maximized, while Windows still believes it is maximized (`WS_MAXIMIZE` stays
/// set, so the restore button and Snap Layouts still say "maximized"). It does not
/// come back when the popover closes, and the screen keeps the stale pixels the
/// window no longer covers.
///
/// **Why it happens.** Traced through the shipped GTK 4.22.4 sources:
///
/// - A popover is a `GtkNative` with its **own** `GdkSurface`, and a popup surface
///   shares its parent's frame clock. `gdk_surface_layout_on_clock` runs
///   `compute_size` for *every* mapped surface on the clock whenever the LAYOUT
///   phase fires — so the popup's layout request drags the toplevel through
///   `gdk_win32_toplevel_compute_size` too.
/// - The toplevel never asked for that layout, so its
///   `impl->next_layout.configured_width/height` are still the zeros
///   `gdk_win32_toplevel_compute_size` left behind last time. With no configured
///   size to honour, `compute_toplevel_size` falls back to what GtkWindow's
///   `compute-size` handler returns — which is `priv->default_width/height`, i.e.
///   the **remembered** size.
/// - `should_remember_size()` is deliberately false while maximized, so that
///   remembered size is still the *pre-maximize* one. It differs from the current
///   size, so `needs_resize` goes true and GDK calls `gdk_win32_surface_resize` —
///   a bare `SetWindowPos`, which shrinks the window without clearing `WS_MAXIMIZE`.
///
/// **Why only on Windows, and only with the native frame.** Under CSD, GTK owns the
/// maximize button, so maximizing goes through `gtk_window_maximize()`, which sets
/// `priv->is_set_maximized` and makes every subsequent `GdkToplevelLayout` carry
/// `maximized`. That flag is what unlocks the branch in `compute_toplevel_size`
/// that clamps the computed size up to the monitor work area, which masks the stale
/// remembered size. Maximizing from the **OS** button never sets the flag, so the
/// clamp never runs. `GTK_CSD=0` is what puts us on that path — which is why this
/// defect is unreachable under CSD and was not waiting to be found by the rest of
/// the GTK-on-Windows world (GTK4Rs/AP-158).
///
/// **Why this fix and not `gtk_window_maximize()`.** Calling it from a
/// `notify::maximized` handler would set the flag and re-enable the clamp, but it
/// also calls `gdk_toplevel_present`, and `gdk_win32_toplevel_present` *first*
/// resizes the surface to the remembered (small) size and only then calls
/// `ShowWindow(SW_MAXIMIZE)` — which Win32 no-ops on an already-maximized window.
/// It would perform the very shrink it was meant to prevent. Worse, the clamp it
/// unlocks is CSD-shaped: it clamps to the **work area**, which is the client size
/// only when GTK draws the frame. With a native frame the client area is the work
/// area *minus the caption*, so the clamp overshoots and pushes the bottom edge
/// under the taskbar.
///
/// Keeping the remembered size correct instead means the fallback path computes the
/// size the window already has, `needs_resize` stays false, and **no resize happens
/// at all** — no clamp, no `present`, no flicker. It costs one comparison per layout
/// pass.
///
/// Nothing needs to restore the remembered size on unmaximize: `should_remember_size`
/// becomes true again the moment the state clears, so GTK resumes maintaining it
/// itself from the first layout pass afterwards. And the OS — not GTK — holds the
/// rectangle a restore returns to (`WINDOWPLACEMENT`), which is why this is safe
/// here and would **not** be on X11/Wayland.
///
/// Wired to `realize` for the same reason as [`track_caption_theme`]: that is where
/// the surface first exists.
pub(crate) fn track_maximized_size(window: &gtk::ApplicationWindow) {
    window.connect_realize(|window| {
        let Some(surface) = window.surface() else {
            return;
        };
        surface.connect_layout(glib::clone!(
            #[weak]
            window,
            move |_, width, height| {
                if let Some((w, h)) = remembered_size_while_maximized(
                    window.is_maximized(),
                    window.default_size(),
                    (width, height),
                ) {
                    window.set_default_size(w, h);
                }
            }
        ));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point of the rule: maximized, the remembered size must be dragged
    /// up to the real one, because a stale remembered size is what GDK-Win32 resizes
    /// the window down to on a popover's layout pass.
    #[test]
    fn maximized_adopts_the_size_the_os_gave_us() {
        assert_eq!(
            remembered_size_while_maximized(true, (1169, 720), (4096, 2089)),
            Some((4096, 2089)),
        );
    }

    /// The other half, and the one that would be a real regression if inverted:
    /// unmaximized, GTK's own bookkeeping owns the remembered size — that value is
    /// what a later restore returns to, so writing over it would make restore land
    /// on the maximized size.
    #[test]
    fn unmaximized_leaves_gtks_own_bookkeeping_alone() {
        assert_eq!(
            remembered_size_while_maximized(false, (1169, 720), (4096, 2089)),
            None,
        );
    }

    /// `set_default_size` queues a resize, and this runs *from* a layout handler, so
    /// an unconditional write would re-enter layout every frame. Converging on
    /// equality is what stops that.
    #[test]
    fn an_already_correct_remembered_size_is_not_rewritten() {
        assert_eq!(
            remembered_size_while_maximized(true, (4096, 2089), (4096, 2089)),
            None,
        );
    }

    /// A surface that has not been allocated yet reports 0. Adopting that would
    /// replace a merely-stale remembered size with a degenerate one.
    #[test]
    fn an_unallocated_surface_is_never_adopted() {
        assert_eq!(
            remembered_size_while_maximized(true, (1169, 720), (0, 0)),
            None
        );
        assert_eq!(
            remembered_size_while_maximized(true, (1169, 720), (4096, 0)),
            None
        );
        assert_eq!(
            remembered_size_while_maximized(true, (1169, 720), (-1, 720)),
            None
        );
    }
}

/// Proves the caption request actually LANDS, by reading it back out of DWM.
///
/// This needs a real toplevel and a live `HWND`, so it cannot be a unit test — and
/// asserting on our own inputs would prove nothing anyway. `DwmGetWindowAttribute`
/// returns what the compositor stored, so a passing test rules out every link in
/// the chain independently: the GDK symbol resolves, the surface yields a non-null
/// handle, DWM accepts attribute 20 at that size, and the stored value matches the
/// desktop probe the rest of the app themes itself from.
#[cfg(all(test, feature = "gtk-integration-tests"))]
mod gtk_integration_tests {
    use super::*;

    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmGetWindowAttribute(
            hwnd: *mut c_void,
            attribute: u32,
            value: *mut c_void,
            value_size: u32,
        ) -> i32;
    }

    /// Read back what DWM is actually holding for `window`'s caption.
    fn stored_caption_dark(window: &gtk::Window) -> bool {
        // SAFETY: the caller realizes the window first, so the surface is live and
        // its pointer is valid for the duration of the call.
        let hwnd =
            unsafe { gdk_win32_surface_get_handle(window.surface().unwrap().to_glib_none().0) };
        assert!(
            !hwnd.is_null(),
            "a realized Win32 surface must yield an HWND"
        );

        let mut stored: i32 = -1;
        // SAFETY: `stored` is a live i32 and `value_size` matches its real size.
        let hr = unsafe {
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                std::ptr::addr_of_mut!(stored).cast::<c_void>(),
                std::mem::size_of::<i32>() as u32,
            )
        };
        assert_eq!(hr, 0, "DwmGetWindowAttribute failed: HRESULT 0x{hr:08x}");
        stored != 0
    }

    #[gtktest::test]
    fn the_caption_follows_the_desktop_theme_and_dwm_stores_it() {
        let window = gtk::Window::new();
        // Disambiguated: `NativeExt` and `WidgetExt` both offer `realize`, and only
        // the widget one creates the surface this test needs.
        gtk::prelude::WidgetExt::realize(&window);
        assert!(
            window.surface().is_some(),
            "a realized toplevel must have a surface for there to be an HWND at all"
        );

        // Both states, explicitly, dark FIRST. DWM's default is already light, so a
        // light-only assertion would pass on a light-themed machine even if the
        // call did nothing at all. Driving dark→light→dark also proves the
        // attribute is settable in both directions, which is what a live desktop
        // theme flip needs — a one-way latch would fail only for dark→light users.
        for expected in [true, false, true] {
            apply_caption_dark(&window, expected);
            assert_eq!(
                stored_caption_dark(&window),
                expected,
                "DWM must store the caption lightness we asked for (dark={expected})",
            );
        }

        // And the entry point the app actually calls resolves to the desktop probe
        // — the same key the editor scheme, toolbar and sidebar follow.
        sync_caption_theme(&window);
        assert_eq!(
            stored_caption_dark(&window),
            crate::palette::desktop_is_dark(),
            "the caption DWM stored must match the desktop probe the app themes from",
        );

        window.destroy();
    }
}
