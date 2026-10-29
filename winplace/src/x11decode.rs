//! The decisions in an X11 place, over the property values as read, so they test on every
//! platform. `x11.rs` reads the properties into a [`RawX11`] and hands it here; nothing
//! in this module touches Xlib, GDK or D-Bus. The CI display runs no window manager, so
//! none of these properties exist there: these tests are the only automated check of the
//! decisions outside `scripts/winplace-rig.sh`.

use crate::geom::Rect;
use crate::Place;
use std::ffi::{c_long, c_ulong};

/// `_NET_WM_DESKTOP`'s value for "on every desktop".
pub(crate) const ALL_DESKTOPS: c_ulong = 0xFFFF_FFFF;

/// The state atoms that are part of a place, as the X server numbered them.
#[derive(Clone, Copy, Debug)]
pub(crate) struct StateAtoms {
    pub shaded: c_ulong,
    pub above: c_ulong,
    pub below: c_ulong,
}

/// A window's X11 properties as read, before anything is decided about them. `None`
/// means the property is absent. Format-32 data is as Xlib hands it over, widened to
/// C `long`s.
#[derive(Clone, Debug, Default)]
pub(crate) struct RawX11 {
    /// `_NET_WM_DESKTOP`'s first item.
    pub desktop: Option<c_long>,
    /// `_KDE_NET_WM_ACTIVITIES`, as text.
    pub activities: Option<String>,
    /// `_NET_WM_STATE`'s atoms.
    pub states: Option<Vec<c_ulong>>,
    /// `_NET_FRAME_EXTENTS`: left, right, top, bottom, in device pixels. Narrowed to
    /// `i32` by the reader, where `c_long` is wider, so this decoder (compiled and tested
    /// on every platform, and `c_long` is 32-bit on Windows) does no same-type conversion
    /// that one platform's lints reject.
    pub frame_extents: Option<Vec<i32>>,
    /// Whether the window manager lists `_NET_FRAME_EXTENTS` in `_NET_SUPPORTED`, so
    /// that the property's absence means something.
    pub frame_extents_supported: bool,
    /// Whether an EWMH window manager is running ([`wm_check_alive`]).
    pub wm_running: bool,
    /// The client area's top-left in root-window device pixels.
    pub client_origin: Option<(i32, i32)>,
    /// The surface's scale factor; device pixels per logical pixel.
    pub scale: i32,
}

/// The X11 parts of a place, decided from `raw` into `place`. `place.maximized` and
/// `place.fullscreen` must already be set; `monitor` is the geometry of the monitor
/// `place.monitor` names, in logical pixels.
pub(crate) fn decode(raw: &RawX11, atoms: StateAtoms, monitor: Option<Rect>, place: &mut Place) {
    if let Some(desktop) = raw.desktop {
        if desktop as c_ulong & ALL_DESKTOPS == ALL_DESKTOPS {
            place.all_desktops = true;
        } else {
            place.desktop = u32::try_from(desktop).ok();
        }
    }
    if let Some(text) = &raw.activities {
        place.activities = crate::geom::split_activities(text);
    }
    if let Some(states) = &raw.states {
        place.shaded = states.contains(&atoms.shaded);
        place.above = states.contains(&atoms.above);
        place.below = states.contains(&atoms.below);
    }
    place.no_border = frameless(raw) && !place.fullscreen && !place.maximized && raw.wm_running;

    // Position: the frame's top-left relative to the monitor the window is on, in
    // logical pixels. Never root coordinates, which change with the monitor layout.
    let (left, top) = match raw.frame_extents.as_deref() {
        Some([l, _r, t, _b, ..]) => (*l, *t),
        _ => (0, 0),
    };
    if let (Some((cx, cy)), Some(m)) = (raw.client_origin, monitor) {
        let scale = raw.scale.max(1);
        place.x = Some((cx - left) / scale - m.x);
        place.y = Some((cy - top) / scale - m.y);
    }
}

/// Whether the window has no frame. An all-zero `_NET_FRAME_EXTENTS` says so outright.
/// KWin gives a window without titlebar and frame no `_NET_FRAME_EXTENTS` at all
/// (MEASURED), which means the same — but only from a window manager that publishes the
/// property at all. One that does not list it in `_NET_SUPPORTED` leaves it absent on
/// every window, framed or not, and a restore would then strip every window's frame.
fn frameless(raw: &RawX11) -> bool {
    match raw.frame_extents.as_deref() {
        Some([0, 0, 0, 0, ..]) => true,
        Some(_) => false,
        None => raw.frame_extents_supported,
    }
}

/// `_NET_WM_STATE` with `extra` added: what GTK just wrote (maximized, full screen) is
/// kept, and an atom already present is not repeated.
pub(crate) fn merged_states(mut current: Vec<c_ulong>, extra: &[c_ulong]) -> Vec<c_ulong> {
    for a in extra {
        if !current.contains(a) {
            current.push(*a);
        }
    }
    current
}

/// Whether an EWMH window manager is running, from `_NET_SUPPORTING_WM_CHECK` read off
/// the root window and then off the window it names. EWMH has the window manager set the
/// same property on that child, pointing at itself: the root's copy outlives a window
/// manager that crashed, the child does not.
pub(crate) fn wm_check_alive(on_root: Option<&[c_long]>, on_child: Option<&[c_long]>) -> bool {
    match (
        on_root.and_then(<[_]>::first),
        on_child.and_then(<[_]>::first),
    ) {
        (Some(&root), Some(&child)) => root != 0 && root == child,
        _ => false,
    }
}

/// Whether `place` has anything the X11 half writes; a place with none is shown with a
/// plain `present`, exactly like a new window.
pub(crate) fn has_x11_parts(place: &Place) -> bool {
    place.desktop.is_some()
        || place.all_desktops
        || !place.activities.is_empty()
        || (place.x.is_some() && place.y.is_some())
        || place.shaded
        || place.above
        || place.below
        || place.no_border
}

/// Whether the session bus can be reached without GIO's X11 autolaunch, which runs
/// `dbus-launch` synchronously and with no bound. GIO looks for the bus in
/// `DBUS_SESSION_BUS_ADDRESS`, then at `$XDG_RUNTIME_DIR/bus`, and only then
/// autolaunches; `address` and `runtime_bus_exists` are the first two.
pub(crate) fn session_bus_without_autolaunch(
    address: Option<&std::ffi::OsStr>,
    runtime_bus_exists: bool,
) -> bool {
    address.is_some_and(|a| !a.is_empty()) || runtime_bus_exists
}

#[cfg(test)]
mod tests {
    use super::*;

    const ATOMS: StateAtoms = StateAtoms {
        shaded: 301,
        above: 302,
        below: 303,
    };
    const MON: Rect = Rect {
        x: 1920,
        y: 0,
        width: 2560,
        height: 1440,
    };

    /// A framed window under a running window manager that publishes frame extents.
    fn framed() -> RawX11 {
        RawX11 {
            frame_extents: Some(vec![4, 4, 36, 4]),
            frame_extents_supported: true,
            wm_running: true,
            scale: 1,
            ..RawX11::default()
        }
    }

    fn decoded(raw: &RawX11, monitor: Option<Rect>, place: Place) -> Place {
        let mut place = place;
        decode(raw, ATOMS, monitor, &mut place);
        place
    }

    #[test]
    fn the_all_desktops_mask_is_all_desktops_and_anything_else_is_one_desktop() {
        let all = RawX11 {
            desktop: Some(0xFFFF_FFFF_u32 as c_long),
            ..framed()
        };
        let p = decoded(&all, None, Place::default());
        assert!(p.all_desktops);
        assert_eq!(p.desktop, None);
        // A server that sign-extends the CARDINAL reads the same.
        let all = RawX11 {
            desktop: Some(-1),
            ..framed()
        };
        assert!(decoded(&all, None, Place::default()).all_desktops);
        let one = RawX11 {
            desktop: Some(2),
            ..framed()
        };
        let p = decoded(&one, None, Place::default());
        assert!(!p.all_desktops);
        assert_eq!(p.desktop, Some(2));
        assert_eq!(decoded(&framed(), None, Place::default()).desktop, None);
    }

    #[test]
    fn each_state_is_read_from_its_own_atom() {
        let raw = RawX11 {
            states: Some(vec![999, ATOMS.below, ATOMS.shaded]),
            ..framed()
        };
        let p = decoded(&raw, None, Place::default());
        assert!(p.shaded && p.below && !p.above);
        let raw = RawX11 {
            states: Some(vec![ATOMS.above]),
            ..framed()
        };
        let p = decoded(&raw, None, Place::default());
        assert!(p.above && !p.shaded && !p.below);
    }

    #[test]
    fn activities_are_split_from_the_property() {
        let raw = RawX11 {
            activities: Some("a,b".into()),
            ..framed()
        };
        assert_eq!(decoded(&raw, None, Place::default()).activities, ["a", "b"]);
    }

    #[test]
    fn an_all_zero_frame_is_no_border_and_a_real_frame_is_not() {
        let zero = RawX11 {
            frame_extents: Some(vec![0, 0, 0, 0]),
            ..framed()
        };
        assert!(decoded(&zero, None, Place::default()).no_border);
        assert!(!decoded(&framed(), None, Place::default()).no_border);
    }

    #[test]
    fn an_absent_frame_is_no_border_only_from_a_window_manager_that_publishes_frames() {
        let kwin = RawX11 {
            frame_extents: None,
            ..framed()
        };
        assert!(decoded(&kwin, None, Place::default()).no_border);
        let other = RawX11 {
            frame_extents_supported: false,
            ..kwin
        };
        assert!(!decoded(&other, None, Place::default()).no_border);
    }

    #[test]
    fn full_screen_maximized_and_no_window_manager_are_never_no_border() {
        let zero = RawX11 {
            frame_extents: Some(vec![0, 0, 0, 0]),
            ..framed()
        };
        for place in [
            Place {
                fullscreen: true,
                ..Place::default()
            },
            Place {
                maximized: true,
                ..Place::default()
            },
        ] {
            assert!(!decoded(&zero, None, place).no_border);
        }
        let no_wm = RawX11 {
            wm_running: false,
            ..zero
        };
        assert!(!decoded(&no_wm, None, Place::default()).no_border);
    }

    #[test]
    fn the_position_is_the_frames_corner_relative_to_the_monitor_in_logical_pixels() {
        // Client area at root (2000·2, 136·2) on a scale-2 surface, frame 4 left, 36 top
        // (in device pixels), monitor at logical x 1920.
        let raw = RawX11 {
            client_origin: Some((4000 + 4, 272 + 36)),
            scale: 2,
            ..framed()
        };
        let p = decoded(&raw, Some(MON), Place::default());
        assert_eq!((p.x, p.y), (Some(2000 - 1920), Some(136)));
        // No monitor, or no client origin: no position.
        let p = decoded(&raw, None, Place::default());
        assert_eq!((p.x, p.y), (None, None));
        let raw = RawX11 {
            client_origin: None,
            ..raw
        };
        let p = decoded(&raw, Some(MON), Place::default());
        assert_eq!((p.x, p.y), (None, None));
    }

    #[test]
    fn a_frameless_window_is_placed_by_its_client_corner() {
        let raw = RawX11 {
            frame_extents: None,
            client_origin: Some((1970, 60)),
            ..framed()
        };
        let p = decoded(&raw, Some(MON), Place::default());
        assert_eq!((p.x, p.y), (Some(50), Some(60)));
    }

    #[test]
    fn merging_keeps_what_is_there_and_adds_each_atom_once() {
        assert_eq!(merged_states(vec![10, 20], &[20, 30]), [10, 20, 30]);
        assert_eq!(merged_states(Vec::new(), &[30]), [30]);
        assert_eq!(merged_states(vec![10], &[]), [10]);
    }

    #[test]
    fn a_window_manager_is_running_only_when_its_check_window_points_at_itself() {
        assert!(wm_check_alive(Some(&[77]), Some(&[77])));
        // The root's copy left behind by a window manager that died.
        assert!(!wm_check_alive(Some(&[77]), None));
        assert!(!wm_check_alive(Some(&[77]), Some(&[78])));
        assert!(!wm_check_alive(None, None));
        assert!(!wm_check_alive(Some(&[0]), Some(&[0])));
        assert!(!wm_check_alive(Some(&[]), Some(&[77])));
    }

    #[test]
    fn a_place_with_only_a_size_and_states_has_no_x11_parts() {
        let plain = Place {
            width: 800,
            height: 600,
            maximized: true,
            fullscreen: true,
            monitor: Some("HDMI-0".into()),
            ..Place::default()
        };
        assert!(!has_x11_parts(&plain));
        // A position needs both halves.
        assert!(!has_x11_parts(&Place {
            x: Some(1),
            ..plain.clone()
        }));
        for part in [
            Place {
                x: Some(1),
                y: Some(2),
                ..plain.clone()
            },
            Place {
                desktop: Some(0),
                ..plain.clone()
            },
            Place {
                all_desktops: true,
                ..plain.clone()
            },
            Place {
                activities: vec!["a".into()],
                ..plain.clone()
            },
            Place {
                shaded: true,
                ..plain.clone()
            },
            Place {
                above: true,
                ..plain.clone()
            },
            Place {
                below: true,
                ..plain.clone()
            },
            Place {
                no_border: true,
                ..plain.clone()
            },
        ] {
            assert!(has_x11_parts(&part), "{part:?}");
        }
    }

    #[test]
    fn the_session_bus_is_reachable_without_autolaunch_from_either_source() {
        use std::ffi::OsStr;
        assert!(session_bus_without_autolaunch(
            Some(OsStr::new("unix:path=/run/user/1/bus")),
            false
        ));
        assert!(session_bus_without_autolaunch(None, true));
        assert!(!session_bus_without_autolaunch(Some(OsStr::new("")), false));
        assert!(!session_bus_without_autolaunch(None, false));
    }
}
