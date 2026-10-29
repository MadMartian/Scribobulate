//! `winplace` — where a GTK4 window sits on the desktop, captured and put back.
//!
//! A [`Place`] is everything about where and how a window sits: its size, maximized and
//! full screen on every platform, and on X11 its virtual desktop, KDE activities,
//! monitor and position on that monitor, and the shaded / kept above / kept below /
//! no-titlebar-and-frame states. [`capture`] reads one off a live window; [`present`]
//! shows a window in one, placed from its first frame rather than moved after it
//! appears.
//!
//! GTK 4 has no API for any of the X11 parts, and GTK 4.6 deletes or overwrites some of
//! them on the first map, so [`present`] owns the order of the writes around
//! `gtk_window_present` — that order is the whole mechanism (`x11.rs`'s header).
//! Wayland, macOS and Windows get size, maximized and full screen only; the X11-only
//! fields of a place are ignored there without error.
//!
//! **Degrading.** Each part is applied on its own and a part that no longer exists falls
//! back to where a new window would go without affecting the rest: an unknown virtual
//! desktop is ignored by the window manager; activities that no longer exist are
//! dropped, and if none remain the window opens on the current activity; a monitor that
//! is gone means no position is written and the size is fitted to the largest monitor
//! still listed; a monitor that is smaller pulls the window back
//! until its title bar is reachable.
//!
//! Knows nothing about the application that uses it.

mod geom;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod win32;
#[cfg(all(unix, not(target_os = "macos")))]
mod x11;
#[cfg(any(test, all(unix, not(target_os = "macos"))))] // X11 only; tested everywhere.
mod x11decode;

use gtk::gdk;
use gtk::prelude::*;

/// Where and how a window sits on the desktop. Every field is optional in the sense
/// that its default means "not recorded": a place read from an older file, or one
/// captured off X11, simply lacks the parts it has no value for.
///
/// Serialises flat, scalars and one array only, so it can be a TOML sub-table.
#[derive(serde::Serialize, serde::Deserialize, PartialEq, Eq, Clone, Debug, Default)]
#[serde(default)]
pub struct Place {
    /// The window's default size (`gtk_window_get_default_size`) in logical pixels: the
    /// size it returns to when it leaves maximized or full screen. `0` means unknown.
    pub width: i32,
    pub height: i32,
    pub maximized: bool,
    pub fullscreen: bool,
    /// The monitor's connector name (`gdk_monitor_get_connector`). Used for full screen
    /// on every platform and for the position on X11.
    pub monitor: Option<String>,
    /// X11: the frame's top-left, relative to `monitor`'s origin, in logical pixels.
    pub x: Option<i32>,
    pub y: Option<i32>,
    /// X11: the one virtual desktop the window is on (0-based). X11 cannot put a window
    /// on several desktops but not all; `all_desktops` covers the other case.
    pub desktop: Option<u32>,
    pub all_desktops: bool,
    /// X11/KDE: the activity UUIDs the window is on. Empty means not recorded; the nil
    /// UUID means every activity.
    pub activities: Vec<String>,
    pub shaded: bool,
    pub above: bool,
    pub below: bool,
    /// Shown without titlebar and frame (KWin's "No Titlebar and Frame").
    pub no_border: bool,
}

/// Read `window`'s place. The window must be realized for the X11 parts and the
/// monitor to be read; an unrealized window yields its size and states only.
pub fn capture(window: &impl IsA<gtk::Window>) -> Place {
    let window = window.upcast_ref::<gtk::Window>();
    let (dw, dh) = window.default_size();
    let place = Place {
        width: if dw > 0 { dw } else { window.width() },
        height: if dh > 0 { dh } else { window.height() },
        maximized: window.is_maximized(),
        fullscreen: window.is_fullscreen(),
        monitor: window
            .surface()
            .and_then(|s| s.display().monitor_at_surface(&s))
            .and_then(|m| m.connector())
            .map(|c| c.to_string()),
        ..Place::default()
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let place = {
        let mut place = place;
        x11::capture(window, &mut place);
        place
    };
    // While another window's full-screen Space is current, GDK reads a zoomed window as
    // not maximized (`macos.rs`'s header). Not for a full-screen window: AppKit calls
    // that one zoomed in its own Space.
    #[cfg(target_os = "macos")]
    let place = Place {
        maximized: place.maximized || (!place.fullscreen && macos::still_zoomed(window)),
        ..place
    };
    // Maximized, GTK's default size is the maximized size here, not the one a restore
    // returns to (`win32.rs`'s header).
    #[cfg(windows)]
    let place = match place
        .maximized
        .then(|| win32::normal_size(window))
        .flatten()
    {
        Some((width, height)) => Place {
            width,
            height,
            ..place
        },
        None => place,
    };
    place
}

/// Follow `window` for the rest of its life so [`capture`] can read its states where the
/// platform misreports them. Call once per window, restored or not — any window can be
/// maximized before it is captured. Does nothing except on macOS (`macos.rs`).
pub fn watch(window: &impl IsA<gtk::Window>) {
    #[cfg(target_os = "macos")]
    macos::watch(window.upcast_ref());
    #[cfg(not(target_os = "macos"))]
    let _ = window;
}

/// Show `window` in `place`, in place of `gtk_window_present`.
///
/// Size and the maximized and full screen states are set before the first map on every
/// platform. On X11 the window is realized first so the remaining parts can be written
/// onto its X window where they survive until the window manager reads them.
pub fn present(window: &impl IsA<gtk::Window>, place: &Place) {
    let window = window.upcast_ref::<gtk::Window>();
    let display = WidgetExt::display(window);
    let listed = monitors(&display);
    let monitor = place.monitor.as_deref().and_then(|name| {
        listed
            .iter()
            .find(|m| m.connector().as_deref() == Some(name))
            .cloned()
    });
    let monitor_rect = monitor.as_ref().map(rect_of);
    // The one place a restored window's size is decided; the application's builder
    // sizes only windows with no place.
    if place.width > 0 && place.height > 0 {
        let listed: Vec<geom::Rect> = listed.iter().map(rect_of).collect();
        let (w, h) = geom::fit_size((place.width, place.height), monitor_rect, &listed);
        window.set_default_size(w, h);
    }
    // With the native frame on Windows a maximized place is maximized through the OS
    // once presented, never by `gtk_window_maximize` (`win32.rs`'s header). The frame
    // is only known once the window is realized.
    #[cfg(windows)]
    let natively = place.maximized && !place.fullscreen && {
        WidgetExt::realize(window);
        win32::native_frame(window)
    };
    #[cfg(not(windows))]
    let natively = false;
    if place.fullscreen {
        match &monitor {
            Some(m) => window.fullscreen_on_monitor(m),
            None => window.fullscreen(),
        }
    } else if place.maximized && !natively {
        window.maximize();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        WidgetExt::realize(window);
        x11::present(window, place, monitor_rect);
    }
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    {
        let _ = monitor_rect;
        window.present();
        // In the same turn as the present, before any layout can run.
        #[cfg(windows)]
        if natively {
            win32::maximize_natively(window);
        }
    }
}

/// Hooks for an application's own tests. Not part of the placing API.
#[cfg(feature = "test-support")]
pub mod testing {
    use gtk::prelude::*;

    /// Resize `window` behind GTK's back the way a window manager does when it
    /// maximizes or full-screens it, for a test display that runs no window manager.
    /// Returns `false` where this cannot be done (anything but X11), where the real
    /// window manager already does it.
    pub fn resize_behind_gtk(window: &impl IsA<gtk::Window>, width: i32, height: i32) -> bool {
        #[cfg(all(unix, not(target_os = "macos")))]
        return crate::x11::resize_behind_gtk(window.upcast_ref(), width, height);
        #[cfg(not(all(unix, not(target_os = "macos"))))]
        {
            let _ = (window, width, height);
            false
        }
    }

    /// Take a maximized `window` out of maximized the way its frame's restore button
    /// does. Returns `false` where the OS draws no such button for it: anything but
    /// Windows, and a client-decorated window there.
    pub fn restore_natively(window: &impl IsA<gtk::Window>) -> bool {
        #[cfg(windows)]
        {
            let window = window.upcast_ref::<gtk::Window>();
            crate::win32::native_frame(window) && crate::win32::restore_natively(window)
        }
        #[cfg(not(windows))]
        {
            let _ = window;
            false
        }
    }
}

/// Every monitor `display` lists.
fn monitors(display: &gdk::Display) -> Vec<gdk::Monitor> {
    let list = display.monitors();
    (0..list.n_items())
        .filter_map(|i| list.item(i).and_then(|o| o.downcast::<gdk::Monitor>().ok()))
        .collect()
}

/// `monitor`'s geometry, in GDK logical pixels.
fn rect_of(monitor: &gdk::Monitor) -> geom::Rect {
    let g = monitor.geometry();
    geom::Rect {
        x: g.x(),
        y: g.y(),
        width: g.width(),
        height: g.height(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A place written by this crate reads back identically, and an empty table reads as
    /// "nothing recorded" rather than failing.
    #[test]
    fn a_place_round_trips_through_toml_and_an_empty_one_is_the_default() {
        let place = Place {
            width: 1111,
            height: 777,
            maximized: true,
            monitor: Some("HDMI-0".into()),
            x: Some(-20),
            y: Some(40),
            desktop: Some(2),
            activities: vec!["a".into(), "b".into()],
            above: true,
            no_border: true,
            ..Place::default()
        };
        let text = toml::to_string(&place).expect("serialises");
        assert_eq!(toml::from_str::<Place>(&text).expect("parses"), place);
        assert_eq!(
            toml::from_str::<Place>("").expect("parses"),
            Place::default()
        );
    }
}
