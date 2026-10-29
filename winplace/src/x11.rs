//! The X11 half of a place: the window properties a window manager reads when it first
//! manages a window, and the order they have to be written in around `present` so they
//! survive GTK 4.6 and reach KWin. MEASURED on a nested KWin 5.27.11 with GTK 4.6.9
//! (`scripts/winplace-rig.sh`). The GDK half of each ordering rule is GTK4Rs/AP-367; the
//! shaded rule is GTK4Rs/AP-368.
//!
//! **Before `present`**, after realize: activities, position (`XMoveWindow` plus
//! `USPosition` in `WM_NORMAL_HINTS`) and the no-border Motif hint. GTK does not touch
//! these at map time.
//!
//! **Under a server grab held across `present`**: the virtual desktop and the above /
//! below states. On the first map `gdk_x11_surface_show` runs `set_initial_hints`, which
//! DELETES `_NET_WM_DESKTOP` and REPLACES `_NET_WM_STATE` with the states GTK knows, then
//! calls `XMapWindow`, with nothing in between an application can hook. KWin reads both
//! once, from current server state, while handling that map (`X11Window::manage`), and
//! never applies a later write as an initial state. The grab keeps KWin from handling
//! the map until the writes are in. Without it the order is a race; the rig could not
//! make that race lose (0 of 20 runs with the grab removed, KWin 5.27.11 on Xvfb), so
//! the rig does not guard the grab, and its removal must not be read as proven safe.
//!
//! **After `present`, until KWin has managed the window**: a bounded wait. GDK rewrites
//! `WM_NORMAL_HINTS` from scratch on its first layout (`gdk_x11_surface_set_geometry_hints`
//! builds the flags from zero), which drops `USPosition`. Whether KWin read the hints
//! before or after that rewrite was a race, MEASURED lost: without the wait the position
//! was ignored and KWin centred the window. Holding the main thread until KWin has set
//! `WM_STATE` on the window means the read has happened before GDK's layout can run.
//!
//! **After KWin has managed it**: shaded, as an EWMH request. Given as an initial
//! state, KWin shaded the window before GTK's first configure and GTK then remembered
//! a 93×36 default size, so the window came back unshaded and tiny (MEASURED). Shading a
//! window that is already managed keeps its size.
//!
//! **Rules while the grab is held**, each of which would deadlock or break it:
//! - only GDK's own `Display*` (a second connection would be frozen by the grab);
//! - no main-loop iteration;
//! - nothing that reaches `gdk_x11_display_ungrab`, which would release it early;
//! - nothing but `present` and the property writes inside it.
//!
//! This module reads and writes; what the properties it reads mean, and the state list
//! it writes, are decided in `x11decode.rs`, which is tested on every platform.
//!
//! Hand-rolled rather than a `gdk4-x11` dependency: a few GDK getters exported from
//! `libgtk-4`, and a handful of Xlib calls.

use crate::geom::{self, Rect};
use crate::x11decode::{self, RawX11, StateAtoms};
use crate::Place;
use gtk::gdk;
use gtk::glib;
use gtk::glib::translate::{FromGlib, ToGlibPtr};
use gtk::prelude::*;
use std::ffi::{c_char, c_int, c_long, c_uchar, c_ulong, c_void, CString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

type XDisplay = c_void;
type XWindow = c_ulong;
type Atom = c_ulong;

extern "C" {
    fn gdk_x11_display_get_type() -> glib::ffi::GType;
    fn gdk_x11_display_get_xdisplay(display: *mut gdk::ffi::GdkDisplay) -> *mut XDisplay;
    fn gdk_x11_display_get_xrootwindow(display: *mut gdk::ffi::GdkDisplay) -> XWindow;
    fn gdk_x11_surface_get_xid(surface: *mut gdk::ffi::GdkSurface) -> XWindow;
    fn gdk_x11_get_xatom_by_name_for_display(
        display: *mut gdk::ffi::GdkDisplay,
        atom_name: *const c_char,
    ) -> Atom;
    fn gdk_x11_display_error_trap_push(display: *mut gdk::ffi::GdkDisplay);
    fn gdk_x11_display_error_trap_pop_ignored(display: *mut gdk::ffi::GdkDisplay);
}

/// Xlib's `XSizeHints` (`X11/Xutil.h`).
#[repr(C)]
#[derive(Default)]
struct XSizeHints {
    flags: c_long,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
    min_width: c_int,
    min_height: c_int,
    max_width: c_int,
    max_height: c_int,
    width_inc: c_int,
    height_inc: c_int,
    min_aspect_x: c_int,
    min_aspect_y: c_int,
    max_aspect_x: c_int,
    max_aspect_y: c_int,
    base_width: c_int,
    base_height: c_int,
    win_gravity: c_int,
}

/// Xlib's `XClientMessageEvent` (`X11/Xlib.h`), padded to the size of an `XEvent`
/// (24 longs), which is what `XSendEvent` reads.
#[repr(C)]
struct XClientMessageEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut XDisplay,
    window: XWindow,
    message_type: Atom,
    format: c_int,
    data: [c_long; 5],
    _pad: [c_long; 12],
}

/// `USPosition`: the user chose this position, so the window manager honours it.
const US_POSITION: c_long = 1 << 0;
const PROP_MODE_REPLACE: c_int = 0;
const ANY_PROPERTY_TYPE: Atom = 0;
const XA_ATOM: Atom = 4;
const XA_CARDINAL: Atom = 6;
const XA_STRING: Atom = 31;
const XA_WINDOW: Atom = 33;
const CLIENT_MESSAGE: c_int = 33;
const SUBSTRUCTURE_NOTIFY_MASK: c_long = 1 << 19;
const SUBSTRUCTURE_REDIRECT_MASK: c_long = 1 << 20;
const SUCCESS: c_int = 0;
/// `_NET_WM_STATE` client message: add the state; the request comes from an application.
const NET_WM_STATE_ADD: c_long = 1;
const SOURCE_APPLICATION: c_long = 1;
/// `_MOTIF_WM_HINTS`: five longs; flag bit 1 says the decorations field is set.
const MWM_HINTS_LEN: usize = 5;
const MWM_HINTS_DECORATIONS: c_long = 1 << 1;
/// How long `present` waits for the window manager to manage the window. KWin takes a
/// few milliseconds; the bound only matters when something is wrong.
const MANAGE_TIMEOUT: Duration = Duration::from_millis(500);
const MANAGE_POLL: Duration = Duration::from_millis(2);

#[link(name = "X11")]
extern "C" {
    fn XGrabServer(display: *mut XDisplay) -> c_int;
    fn XUngrabServer(display: *mut XDisplay) -> c_int;
    fn XFlush(display: *mut XDisplay) -> c_int;
    fn XFree(data: *mut c_void) -> c_int;
    fn XMoveWindow(display: *mut XDisplay, w: XWindow, x: c_int, y: c_int) -> c_int;
    #[cfg(feature = "test-support")]
    fn XResizeWindow(
        display: *mut XDisplay,
        w: XWindow,
        width: std::ffi::c_uint,
        height: std::ffi::c_uint,
    ) -> c_int;
    fn XGetWMNormalHints(
        display: *mut XDisplay,
        w: XWindow,
        hints: *mut XSizeHints,
        supplied: *mut c_long,
    ) -> c_int;
    fn XSetWMNormalHints(display: *mut XDisplay, w: XWindow, hints: *mut XSizeHints);
    fn XSendEvent(
        display: *mut XDisplay,
        w: XWindow,
        propagate: c_int,
        event_mask: c_long,
        event: *mut XClientMessageEvent,
    ) -> c_int;
    fn XChangeProperty(
        display: *mut XDisplay,
        w: XWindow,
        property: Atom,
        type_: Atom,
        format: c_int,
        mode: c_int,
        data: *const c_uchar,
        nelements: c_int,
    ) -> c_int;
    #[allow(clippy::too_many_arguments)] // Xlib's own signature.
    fn XGetWindowProperty(
        display: *mut XDisplay,
        w: XWindow,
        property: Atom,
        long_offset: c_long,
        long_length: c_long,
        delete: c_int,
        req_type: Atom,
        actual_type: *mut Atom,
        actual_format: *mut c_int,
        nitems: *mut c_ulong,
        bytes_after: *mut c_ulong,
        prop: *mut *mut c_uchar,
    ) -> c_int;
    #[allow(clippy::too_many_arguments)] // Xlib's own signature.
    fn XTranslateCoordinates(
        display: *mut XDisplay,
        src_w: XWindow,
        dest_w: XWindow,
        src_x: c_int,
        src_y: c_int,
        dest_x: *mut c_int,
        dest_y: *mut c_int,
        child: *mut XWindow,
    ) -> c_int;
}

/// One X11 toplevel: GDK's own connection, the root window and the window's XID,
/// borrowed from GObjects held alive for the life of this value.
struct Win {
    gdk_display: gdk::Display,
    xdisplay: *mut XDisplay,
    root: XWindow,
    xid: XWindow,
    scale: i32,
}

/// The atoms this module uses, interned once per call (and always before a grab).
struct Atoms {
    desktop: Atom,
    state: Atom,
    shaded: Atom,
    above: Atom,
    below: Atom,
    activities: Atom,
    motif: Atom,
    frame_extents: Atom,
    wm_state: Atom,
    wm_check: Atom,
    supported: Atom,
}

impl Atoms {
    fn state_atoms(&self) -> StateAtoms {
        StateAtoms {
            shaded: self.shaded,
            above: self.above,
            below: self.below,
        }
    }
}

impl Win {
    /// `None` when the display is not X11 (Wayland in the same build) or the window has
    /// no surface yet.
    fn of(window: &gtk::Window) -> Option<Win> {
        let surface = window.surface()?;
        let gdk_display = surface.display();
        // SAFETY: a GType getter with no preconditions.
        let x11 = unsafe { glib::Type::from_glib(gdk_x11_display_get_type()) };
        if !gdk_display.type_().is_a(x11) {
            return None;
        }
        let display_ptr: *mut gdk::ffi::GdkDisplay = gdk_display.to_glib_none().0;
        let surface_ptr: *mut gdk::ffi::GdkSurface = surface.to_glib_none().0;
        // SAFETY: both objects are alive for this call and the display was just checked
        // to be a GdkX11Display, so its surfaces are X11 surfaces — the one precondition
        // the three getters have.
        let (xdisplay, root, xid) = unsafe {
            (
                gdk_x11_display_get_xdisplay(display_ptr),
                gdk_x11_display_get_xrootwindow(display_ptr),
                gdk_x11_surface_get_xid(surface_ptr),
            )
        };
        if xdisplay.is_null() || xid == 0 {
            return None;
        }
        Some(Win {
            gdk_display,
            xdisplay,
            root,
            xid,
            scale: surface.scale_factor().max(1),
        })
    }

    fn atom(&self, name: &str) -> Atom {
        let Ok(name) = CString::new(name) else {
            return 0;
        };
        let display: *mut gdk::ffi::GdkDisplay = self.gdk_display.to_glib_none().0;
        // SAFETY: the display is a live GdkX11Display and `name` is NUL-terminated.
        unsafe { gdk_x11_get_xatom_by_name_for_display(display, name.as_ptr()) }
    }

    fn atoms(&self) -> Atoms {
        Atoms {
            desktop: self.atom("_NET_WM_DESKTOP"),
            state: self.atom("_NET_WM_STATE"),
            shaded: self.atom("_NET_WM_STATE_SHADED"),
            above: self.atom("_NET_WM_STATE_ABOVE"),
            below: self.atom("_NET_WM_STATE_BELOW"),
            activities: self.atom("_KDE_NET_WM_ACTIVITIES"),
            motif: self.atom("_MOTIF_WM_HINTS"),
            frame_extents: self.atom("_NET_FRAME_EXTENTS"),
            wm_state: self.atom("WM_STATE"),
            wm_check: self.atom("_NET_SUPPORTING_WM_CHECK"),
            supported: self.atom("_NET_SUPPORTED"),
        }
    }

    /// A format-32 property of `window` as `c_long`s (Xlib widens format-32 data to
    /// `long`), or `None` when it is absent or not format 32.
    fn read_longs(&self, window: XWindow, property: Atom, req_type: Atom) -> Option<Vec<c_long>> {
        let bytes = self.read(window, property, req_type, 32)?;
        Some(
            bytes
                .chunks_exact(std::mem::size_of::<c_long>())
                .map(|c| c_long::from_ne_bytes(c.try_into().unwrap_or_default()))
                .collect(),
        )
    }

    /// A format-8 property of this window as text, or `None` when it is absent.
    fn read_string(&self, property: Atom) -> Option<String> {
        let bytes = self.read(self.xid, property, ANY_PROPERTY_TYPE, 8)?;
        Some(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Fetch `property` whole as owned bytes, freeing Xlib's copy. `None` when it is
    /// absent or not of `format`.
    fn read(
        &self,
        window: XWindow,
        property: Atom,
        req_type: Atom,
        format: c_int,
    ) -> Option<Vec<u8>> {
        let mut actual_type: Atom = 0;
        let mut actual_format: c_int = 0;
        let mut nitems: c_ulong = 0;
        let mut after: c_ulong = 0;
        let mut data: *mut c_uchar = std::ptr::null_mut();
        // SAFETY: GDK's live connection and a window on its display; the out-parameters
        // are locals. A length of 1024 longs covers every property read here.
        let status = unsafe {
            XGetWindowProperty(
                self.xdisplay,
                window,
                property,
                0,
                1024,
                0,
                req_type,
                &mut actual_type,
                &mut actual_format,
                &mut nitems,
                &mut after,
                &mut data,
            )
        };
        if status != SUCCESS || data.is_null() {
            return None;
        }
        let out = (actual_type != 0 && actual_format == format).then(|| {
            let n = nitems as usize;
            // Format-32 items are C longs in client memory; format 8 is bytes.
            let len = if format == 32 {
                n * std::mem::size_of::<c_long>()
            } else {
                n
            };
            // SAFETY: Xlib allocated `len` bytes of data for this format and count.
            unsafe { std::slice::from_raw_parts(data, len) }.to_vec()
        });
        // SAFETY: `data` was allocated by Xlib for this call and is not used again.
        unsafe { XFree(data.cast()) };
        out
    }

    fn write_longs(&self, property: Atom, type_: Atom, values: &[c_long]) {
        // SAFETY: format-32 data is an array of C longs, as Xlib requires.
        unsafe {
            XChangeProperty(
                self.xdisplay,
                self.xid,
                property,
                type_,
                32,
                PROP_MODE_REPLACE,
                values.as_ptr().cast(),
                values.len() as c_int,
            );
        }
    }

    fn write_string(&self, property: Atom, value: &str) {
        // SAFETY: format-8 data of `len` bytes.
        unsafe {
            XChangeProperty(
                self.xdisplay,
                self.xid,
                property,
                XA_STRING,
                8,
                PROP_MODE_REPLACE,
                value.as_ptr(),
                value.len() as c_int,
            );
        }
    }

    /// Ask the window manager to add `state` to the window (EWMH `_NET_WM_STATE`).
    fn request_state(&self, atoms: &Atoms, state: Atom) {
        let mut event = XClientMessageEvent {
            type_: CLIENT_MESSAGE,
            serial: 0,
            send_event: 1,
            display: self.xdisplay,
            window: self.xid,
            message_type: atoms.state,
            format: 32,
            data: [NET_WM_STATE_ADD, state as c_long, 0, SOURCE_APPLICATION, 0],
            _pad: [0; 12],
        };
        // SAFETY: GDK's live connection; the event is a local of XEvent's size.
        unsafe {
            XSendEvent(
                self.xdisplay,
                self.root,
                0,
                SUBSTRUCTURE_REDIRECT_MASK | SUBSTRUCTURE_NOTIFY_MASK,
                &mut event,
            );
        }
    }

    /// Whether an EWMH window manager is running on this screen: the root's
    /// `_NET_SUPPORTING_WM_CHECK` names a window that still carries the same property
    /// (`x11decode::wm_check_alive`). Never called under the grab: the error trap syncs.
    fn wm_running(&self, atoms: &Atoms) -> bool {
        let on_root = self.read_longs(self.root, atoms.wm_check, XA_WINDOW);
        let Some(&child) = on_root.as_deref().and_then(<[_]>::first) else {
            return false;
        };
        if child == 0 {
            return false;
        }
        // The window a crashed window manager named no longer exists, and reading off
        // it raises BadWindow, which GDK treats as fatal unless trapped.
        let display: *mut gdk::ffi::GdkDisplay = self.gdk_display.to_glib_none().0;
        // SAFETY: a live GdkX11Display; the trap is pushed and popped around one read.
        unsafe { gdk_x11_display_error_trap_push(display) };
        let on_child = self.read_longs(child as XWindow, atoms.wm_check, XA_WINDOW);
        // SAFETY: as above; pops the trap pushed just before.
        unsafe { gdk_x11_display_error_trap_pop_ignored(display) };
        x11decode::wm_check_alive(on_root.as_deref(), on_child.as_deref())
    }

    /// Whether the window manager lists `atom` in the root's `_NET_SUPPORTED`.
    fn supports(&self, atoms: &Atoms, atom: Atom) -> bool {
        self.read_longs(self.root, atoms.supported, XA_ATOM)
            .is_some_and(|v| v.iter().any(|a| *a as Atom == atom))
    }

    /// Block until the window manager has managed the window (it sets `WM_STATE`), for
    /// at most [`MANAGE_TIMEOUT`]. Never iterates the main loop: that is the point, since
    /// GDK's first layout would rewrite the size hints (this module's header).
    fn wait_until_managed(&self, atoms: &Atoms) -> bool {
        let deadline = Instant::now() + MANAGE_TIMEOUT;
        loop {
            if self
                .read(self.xid, atoms.wm_state, ANY_PROPERTY_TYPE, 32)
                .is_some()
            {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(MANAGE_POLL);
        }
    }

    /// Where the window's client area is, in root-window device pixels.
    fn client_origin(&self) -> Option<(i32, i32)> {
        let (mut x, mut y, mut child): (c_int, c_int, XWindow) = (0, 0, 0);
        // SAFETY: GDK's live connection, two windows it knows, local out-parameters.
        let same = unsafe {
            XTranslateCoordinates(
                self.xdisplay,
                self.xid,
                self.root,
                0,
                0,
                &mut x,
                &mut y,
                &mut child,
            )
        };
        (same != 0).then_some((x, y))
    }

    fn flush(&self) {
        // SAFETY: GDK's live connection.
        unsafe { XFlush(self.xdisplay) };
    }
}

/// The X11-only parts of `window`'s place, read into `place`. Leaves `place` untouched
/// when the display is not X11. This reads the properties; `x11decode` decides what
/// they mean.
pub(crate) fn capture(window: &gtk::Window, place: &mut Place) {
    let Some(win) = Win::of(window) else { return };
    let atoms = win.atoms();
    let raw = RawX11 {
        desktop: win
            .read_longs(win.xid, atoms.desktop, XA_CARDINAL)
            .and_then(|v| v.first().copied()),
        activities: win.read_string(atoms.activities),
        states: win
            .read_longs(win.xid, atoms.state, XA_ATOM)
            .map(|v| v.into_iter().map(|a| a as Atom).collect()),
        // An extent too wide for `i32` is one no window manager publishes: unreadable.
        frame_extents: win
            .read_longs(win.xid, atoms.frame_extents, XA_CARDINAL)
            .and_then(|v| v.into_iter().map(|e| i32::try_from(e).ok()).collect()),
        frame_extents_supported: win.supports(&atoms, atoms.frame_extents),
        wm_running: win.wm_running(&atoms),
        client_origin: win.client_origin(),
        scale: win.scale,
    };
    let monitor = place
        .monitor
        .as_deref()
        .and_then(|n| monitor_named(&win.gdk_display, n))
        .map(|m| crate::rect_of(&m));
    x11decode::decode(&raw, atoms.state_atoms(), monitor, place);
}

/// Show `window` in `place`: the writes before, around and after `gtk_window_present`
/// in the order this module's header gives. `monitor` is the saved monitor if it still
/// exists. Falls back to a plain `present` when the display is not X11 or the place has
/// no X11 parts.
pub(crate) fn present(window: &gtk::Window, place: &Place, monitor: Option<Rect>) {
    let Some(win) = Win::of(window).filter(|_| x11decode::has_x11_parts(place)) else {
        window.present();
        return;
    };
    let atoms = win.atoms();
    before_present(&win, &atoms, place, monitor, window.default_size().0);

    let desktop = if place.all_desktops {
        Some(x11decode::ALL_DESKTOPS)
    } else {
        place.desktop.map(c_ulong::from)
    };
    let extra: Vec<Atom> = [(place.above, atoms.above), (place.below, atoms.below)]
        .into_iter()
        .filter_map(|(on, atom)| on.then_some(atom))
        .collect();

    // SAFETY: GDK's own connection; the grab is released below on every path, and
    // nothing between grab and ungrab iterates the main loop or reaches
    // gdk_x11_display_ungrab (this module's header).
    unsafe { XGrabServer(win.xdisplay) };
    window.present();
    if let Some(d) = desktop {
        win.write_longs(atoms.desktop, XA_CARDINAL, &[d as c_long]);
    }
    if !extra.is_empty() {
        // Read-modify-write: keep what GTK just wrote (maximized, full screen).
        let current = win
            .read_longs(win.xid, atoms.state, XA_ATOM)
            .unwrap_or_default()
            .into_iter()
            .map(|a| a as Atom)
            .collect();
        let states: Vec<c_long> = x11decode::merged_states(current, &extra)
            .into_iter()
            .map(|a| a as c_long)
            .collect();
        win.write_longs(atoms.state, XA_ATOM, &states);
    }
    // SAFETY: as above. Neither call flushes on its own.
    unsafe { XUngrabServer(win.xdisplay) };
    win.flush();

    if !win.wm_running(&atoms) {
        return;
    }
    if !win.wait_until_managed(&atoms) {
        log::warn!(
            "window place: the window manager did not manage the window within {} ms; \
             its position may not have been applied",
            MANAGE_TIMEOUT.as_millis()
        );
        return;
    }
    if place.shaded {
        win.request_state(&atoms, atoms.shaded);
        win.flush();
    }
}

/// The writes GTK leaves alone at map time: activities, position and the no-border
/// hint, made after realize and before `present`.
fn before_present(win: &Win, atoms: &Atoms, place: &Place, monitor: Option<Rect>, width: i32) {
    if !place.activities.is_empty() {
        let running = running_activities();
        let keep = geom::surviving_activities(&place.activities, running.as_deref());
        if keep.is_empty() {
            log::info!(
                "window place: none of the saved activities exist; opening on the current one"
            );
        } else {
            // STRING, not UTF8_STRING: KWin rejects a type mismatch.
            win.write_string(atoms.activities, &keep.join(","));
        }
    }

    if let (Some(m), Some(dx), Some(dy)) = (monitor, place.x, place.y) {
        let (x, y) = geom::frame_origin(m, (dx, dy), width);
        let (x, y) = (x * win.scale, y * win.scale);
        let mut hints = XSizeHints::default();
        let mut supplied: c_long = 0;
        // SAFETY: GDK's live connection and its own window; `hints` is a local of the
        // layout Xlib expects. A failed read leaves the zeroed default, which is fine:
        // GDK rewrites the size fields on its first layout anyway.
        unsafe {
            XGetWMNormalHints(win.xdisplay, win.xid, &mut hints, &mut supplied);
            hints.flags |= US_POSITION;
            hints.x = x;
            hints.y = y;
            XSetWMNormalHints(win.xdisplay, win.xid, &mut hints);
            XMoveWindow(win.xdisplay, win.xid, x, y);
        }
    }

    if place.no_border {
        let mut hints = win
            .read_longs(win.xid, atoms.motif, atoms.motif)
            .filter(|v| v.len() >= MWM_HINTS_LEN)
            .unwrap_or_else(|| vec![0; MWM_HINTS_LEN]);
        hints[0] |= MWM_HINTS_DECORATIONS;
        hints[2] = 0;
        win.write_longs(atoms.motif, atoms.motif, &hints[..MWM_HINTS_LEN]);
    }
}

/// The monitor whose connector is `name`.
fn monitor_named(display: &gdk::Display, name: &str) -> Option<gdk::Monitor> {
    crate::monitors(display)
        .into_iter()
        .find(|m| m.connector().as_deref() == Some(name))
}

/// Set once the activity manager has failed to answer, so a restore of several windows
/// pays for that at most once rather than once per window.
static ACTIVITY_MANAGER_UNREACHABLE: AtomicBool = AtomicBool::new(false);

/// The running KDE activities, or `None` when the activity manager cannot be asked
/// (not KDE, no session bus). Synchronous, at startup, for a restored window that has
/// activities, and bounded: never through GIO's X11 autolaunch of a session bus
/// (`x11decode::session_bus_without_autolaunch`), never auto-starting the manager on a
/// desktop that does not run it, a 500 ms reply timeout, and not asked again by this
/// process once it has failed to answer.
fn running_activities() -> Option<Vec<String>> {
    const TIMEOUT_MS: i32 = 500;
    if ACTIVITY_MANAGER_UNREACHABLE.load(Ordering::Relaxed) {
        return None;
    }
    let runtime_bus = std::env::var_os("XDG_RUNTIME_DIR")
        .is_some_and(|dir| std::path::Path::new(&dir).join("bus").exists());
    let address = std::env::var_os("DBUS_SESSION_BUS_ADDRESS");
    if !x11decode::session_bus_without_autolaunch(address.as_deref(), runtime_bus) {
        return None;
    }
    let list = list_activities(TIMEOUT_MS);
    if list.is_none() {
        ACTIVITY_MANAGER_UNREACHABLE.store(true, Ordering::Relaxed);
    }
    list
}

fn list_activities(timeout_ms: i32) -> Option<Vec<String>> {
    let bus =
        gtk::gio::bus_get_sync(gtk::gio::BusType::Session, gtk::gio::Cancellable::NONE).ok()?;
    let reply = bus
        .call_sync(
            Some("org.kde.ActivityManager"),
            "/ActivityManager/Activities",
            "org.kde.ActivityManager.Activities",
            "ListActivities",
            None,
            Some(glib::VariantTy::new("(as)").ok()?),
            gtk::gio::DBusCallFlags::NO_AUTO_START,
            timeout_ms,
            gtk::gio::Cancellable::NONE,
        )
        .ok()?;
    let (list,): (Vec<String>,) = reply.get()?;
    Some(list)
}

/// Resize `window`'s X window directly, as a window manager does when it maximizes
/// one. Test support: with no window manager running nothing else ever changes a
/// maximized window's size. `false` when the display is not X11.
#[cfg(feature = "test-support")]
pub(crate) fn resize_behind_gtk(window: &gtk::Window, width: i32, height: i32) -> bool {
    let Some(win) = Win::of(window) else {
        return false;
    };
    let (w, h) = (
        (width * win.scale) as std::ffi::c_uint,
        (height * win.scale) as std::ffi::c_uint,
    );
    // SAFETY: GDK's live connection and its own window.
    unsafe { XResizeWindow(win.xdisplay, win.xid, w, h) };
    win.flush();
    true
}
