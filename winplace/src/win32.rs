//! The size a maximized window returns to, read from Windows rather than from GTK.
//!
//! [`crate::capture`] saves a window's default size because that is the size it comes
//! back to on leaving maximized. On Windows that number is wrong while the window is
//! maximized: the application's native-frame repair (`platform/win32/frame.rs`,
//! `track_maximized_size`) sets the default size to the maximized size for as long as
//! the window stays maximized, because otherwise any popover's layout pass shrinks the
//! window back to the remembered size without clearing `WS_MAXIMIZE` (GTK4Rs/AP-158).
//! That repair is safe only because GTK never needs the remembered size there: the OS,
//! not GTK, holds the rectangle a restore returns to, as the window's placement
//! (`WINDOWPLACEMENT::rcNormalPosition`). So that is where this reads it.
//!
//! **Minimized, the window is no longer maximized to GTK.** Minimizing a maximized window
//! clears `WS_MAXIMIZE` while the default size is still the maximized size; the placement
//! record keeps `WPF_RESTORETOMAXIMIZED` and the restore rectangle, so the maximized
//! state is read from there too ([`crate::geom::saved_state`]).
//!
//! `rcNormalPosition` is the OUTER rectangle in physical pixels. GTK's default size is
//! the client area in logical pixels, so the frame a normal window of this style has at
//! this DPI is taken off, and the remainder divided by the scale GDK applies to the
//! surface. That scale is measured — the client area's physical width over the
//! surface's logical width — rather than assumed, because `gdk_surface_get_scale` is
//! newer than the GTK floor. On GDK-Win32 4.22.4 the measurement always equals
//! `gdk_surface_get_scale_factor`: GDK scales a Win32 surface by `dpi / 96` in INTEGER
//! division (`gdkdisplay-win32.c:1241-1288`, `gdksurface-win32.c:2682-2702`), so 125%
//! and 150% are scale 1 and 200% is the first scale 2. That is also why a minimized
//! window, whose client area is empty, falls back to the scale factor and never to
//! `GetDpiForWindow / 96`, which would shrink the size at a fractional scale.
//!
//! **Restoring maximized goes through the OS too.** `gtk_window_maximize` latches
//! `is_set_maximized` for good (GTK 4.22.4 `gtkwindow.c:5649`/`5692`; nothing clears it),
//! so every later `GdkToplevelLayout` still says maximized, and after the frame's restore
//! button `gdk_win32_toplevel_compute_size`'s work-area clamp (`gdksurface-win32.c:2779-2810`)
//! grows the window back to the work area, past the taskbar. A window maximized by
//! [`maximize_natively`] never sets the latch, exactly like one maximized by its own
//! maximize button, which already restores correctly (GTK4Rs/AP-158).
//!
//! The restore half is for the native frame (`GTK_CSD=0`) only: under client-side
//! decorations GTK owns the maximize button, so it keeps `gtk_window_maximize`. The size
//! half applies under both, since the repair rewrites the default size under both.

use gtk::glib::translate::ToGlibPtr;
use gtk::prelude::*;
use std::ffi::c_void;

const GWL_STYLE: i32 = -16;
const GWL_EXSTYLE: i32 = -20;
const WS_MAXIMIZE: u32 = 0x0100_0000;
const WS_MINIMIZE: u32 = 0x2000_0000;
const SW_MAXIMIZE: i32 = 3;
const SW_SHOWMINIMIZED: u32 = 2;
const WPF_RESTORETOMAXIMIZED: u32 = 0x0002;
#[cfg(feature = "test-support")]
const WM_SYSCOMMAND: u32 = 0x0112;
#[cfg(feature = "test-support")]
const SC_RESTORE: usize = 0xF120;

#[repr(C)]
#[derive(Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
#[derive(Default)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
#[derive(Default)]
struct WindowPlacement {
    length: u32,
    flags: u32,
    show_cmd: u32,
    min_position: Point,
    max_position: Point,
    normal_position: Rect,
}

#[link(name = "gtk-4")]
extern "C" {
    fn gdk_win32_surface_get_handle(surface: *mut gtk::gdk::ffi::GdkSurface) -> *mut c_void;
}

#[link(name = "user32")]
extern "system" {
    fn GetWindowPlacement(hwnd: *mut c_void, placement: *mut WindowPlacement) -> i32;
    fn GetWindowLongW(hwnd: *mut c_void, index: i32) -> i32;
    fn GetDpiForWindow(hwnd: *mut c_void) -> u32;
    fn GetClientRect(hwnd: *mut c_void, rect: *mut Rect) -> i32;
    fn ShowWindow(hwnd: *mut c_void, cmd: i32) -> i32;
    #[cfg(feature = "test-support")] // only `restore_natively`, a test helper, sends it
    fn SendMessageW(hwnd: *mut c_void, msg: u32, wparam: usize, lparam: isize) -> isize;
    fn AdjustWindowRectExForDpi(
        rect: *mut Rect,
        style: u32,
        menu: i32,
        ex_style: u32,
        dpi: u32,
    ) -> i32;
}

/// Whether `window` wears the native Win32 frame rather than GTK's client-side
/// decorations, which GTK marks with the `csd` style class once the window is realized.
pub(crate) fn native_frame(window: &gtk::Window) -> bool {
    !window.has_css_class("csd")
}

/// `window`'s `HWND`, once it is realized.
fn hwnd(window: &gtk::Window) -> Option<*mut c_void> {
    let surface = window.surface()?;
    // SAFETY: `surface` is a live GdkSurface owned by `window`, and on Windows it is
    // always a GdkWin32Surface, the only backend GTK offers there.
    let hwnd = unsafe { gdk_win32_surface_get_handle(surface.to_glib_none().0) };
    (!hwnd.is_null()).then_some(hwnd)
}

/// Maximize a presented `window` the way its frame's maximize button does.
pub(crate) fn maximize_natively(window: &gtk::Window) {
    if let Some(hwnd) = hwnd(window) {
        // SAFETY: `hwnd` is a live window owned by this thread.
        unsafe { ShowWindow(hwnd, SW_MAXIMIZE) };
    }
}

/// Restore `window` from maximized the way its frame's restore button does, for a
/// test. `false` when it has no `HWND`.
#[cfg(feature = "test-support")]
pub(crate) fn restore_natively(window: &gtk::Window) -> bool {
    let Some(hwnd) = hwnd(window) else {
        return false;
    };
    // SAFETY: `hwnd` is a live window owned by this thread, so the message is handled
    // synchronously by its own window procedure.
    unsafe { SendMessageW(hwnd, WM_SYSCOMMAND, SC_RESTORE, 0) };
    true
}

/// `window`'s placement record, or `None` when Windows cannot say (no surface yet, or the
/// call failed).
pub(crate) fn placement(window: &gtk::Window) -> Option<crate::geom::Placement> {
    let surface = window.surface()?;
    let hwnd = hwnd(window)?;
    let mut placement = WindowPlacement {
        length: std::mem::size_of::<WindowPlacement>() as u32,
        ..WindowPlacement::default()
    };
    // SAFETY: `hwnd` is non-null and `placement.length` is set as the call requires.
    if unsafe { GetWindowPlacement(hwnd, &mut placement) } == 0 {
        return None;
    }
    Some(crate::geom::Placement {
        minimized: placement.show_cmd == SW_SHOWMINIMIZED,
        restores_maximized: placement.flags & WPF_RESTORETOMAXIMIZED != 0,
        normal_size: normal_size(hwnd, &surface, &placement.normal_position),
    })
}

/// The logical client size of a window whose restore rectangle is `outer`, or `None` when
/// a call failed.
fn normal_size(hwnd: *mut c_void, surface: &gtk::gdk::Surface, outer: &Rect) -> Option<(i32, i32)> {
    // The frame of a NORMAL window of this style: a maximized or minimized one's is not
    // the frame the restore rectangle was measured with.
    // SAFETY: plain reads of a valid window's longs and DPI.
    let (style, ex_style, dpi) = unsafe {
        (
            GetWindowLongW(hwnd, GWL_STYLE) as u32 & !(WS_MAXIMIZE | WS_MINIMIZE),
            GetWindowLongW(hwnd, GWL_EXSTYLE) as u32,
            GetDpiForWindow(hwnd),
        )
    };
    let mut frame = Rect::default();
    // SAFETY: `frame` outlives the call; a zero DPI is rejected by the call itself.
    if dpi == 0 || unsafe { AdjustWindowRectExForDpi(&mut frame, style, 0, ex_style, dpi) } == 0 {
        return None;
    }
    let width = (outer.right - outer.left) - (frame.right - frame.left);
    let height = (outer.bottom - outer.top) - (frame.bottom - frame.top);
    // The scale GDK applies to this surface, measured rather than taken from the integer
    // `scale_factor()`, which is only its ceiling when GDK scales fractionally.
    let mut client = Rect::default();
    // SAFETY: `hwnd` is a live window and `client` outlives the call.
    let client_width = if unsafe { GetClientRect(hwnd, &mut client) } != 0 {
        client.right - client.left
    } else {
        0
    };
    let scale = crate::geom::device_scale(client_width, surface.width(), surface.scale_factor());
    crate::geom::to_logical((width, height), scale)
}
