//! The decisions behind a restore, over plain numbers and strings so they test without a
//! display. Every GDK and X11 call lives in `lib.rs` and `x11.rs`; nothing here touches
//! either.

#[cfg(any(test, all(unix, not(target_os = "macos"))))] // X11 only; tested everywhere.
/// How much of the window has to stay on its monitor, in logical pixels, when the
/// monitor it is restored onto is now smaller than the place it was saved at. Enough to
/// grab the title bar and drag the window back.
pub(crate) const KEEP_REACHABLE: i32 = 48;

/// A rectangle in GDK logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[cfg(any(test, all(unix, not(target_os = "macos"))))] // X11 only; tested everywhere.
/// Where a window's frame goes on `monitor`, given the offset from the monitor's origin
/// it was saved at and the window's width.
///
/// A position that still leaves the title bar reachable is kept exactly, even if part of
/// the window hangs off the monitor: the reader may have put it there. Only a position
/// that would lose the title bar is pulled back, by the least distance that makes
/// [`KEEP_REACHABLE`] pixels of it reachable again.
pub(crate) fn frame_origin(monitor: Rect, offset: (i32, i32), width: i32) -> (i32, i32) {
    let (dx, dy) = offset;
    let keep_x = KEEP_REACHABLE.min(width.max(1));
    let min_x = -(width - keep_x).max(0);
    let max_x = (monitor.width - keep_x).max(0);
    let max_y = (monitor.height - KEEP_REACHABLE).max(0);
    (
        monitor.x + dx.clamp(min_x.min(max_x), max_x),
        monitor.y + dy.clamp(0, max_y),
    )
}

/// The size to open at: the saved size, but never larger than the monitor it opens on.
/// A narrower request is still only a request; GTK grows it back to the content's
/// minimum, so this never makes a window smaller than it can be drawn.
///
/// `saved` is the monitor the place names, if it is still listed. When it is not — the
/// laptop undocked from the screen the window was saved on, or a platform that names no
/// connector — the window manager picks the monitor, so the size is fitted to the
/// largest width and largest height among `listed`: it refuses only a size that no
/// monitor here can show, and never narrows a window the reader was going to put on
/// the big screen. Nothing listed means nothing to fit to.
pub(crate) fn fit_size(size: (i32, i32), saved: Option<Rect>, listed: &[Rect]) -> (i32, i32) {
    let (w, h) = size;
    let usable = |m: &Rect| m.width > 0 && m.height > 0;
    if let Some(m) = saved.filter(usable) {
        return (w.min(m.width), h.min(m.height));
    }
    let widest = listed.iter().filter(|m| usable(m)).map(|m| m.width).max();
    let tallest = listed.iter().filter(|m| usable(m)).map(|m| m.height).max();
    match (widest, tallest) {
        (Some(mw), Some(mh)) => (w.min(mw), h.min(mh)),
        _ => (w, h),
    }
}

#[cfg(any(test, windows))] // Windows only; tested everywhere.
/// Device pixels per logical pixel as GDK applies them to a surface: its client area's
/// physical width over its logical width. Right whether GDK's scale is an integer or a
/// fraction; the integer `scale_factor` stands in when either width is unknown.
pub(crate) fn device_scale(physical_width: i32, logical_width: i32, scale_factor: i32) -> f64 {
    if physical_width > 0 && logical_width > 0 {
        f64::from(physical_width) / f64::from(logical_width)
    } else {
        f64::from(scale_factor.max(1))
    }
}

#[cfg(any(test, windows))] // Windows only; tested everywhere.
/// A physical size in logical pixels at `scale`, rounded; `None` for an empty size.
pub(crate) fn to_logical(physical: (i32, i32), scale: f64) -> Option<(i32, i32)> {
    let (w, h) = physical;
    let logical = |v: i32| (f64::from(v) / scale).round() as i32;
    (w > 0 && h > 0 && scale > 0.0).then(|| (logical(w), logical(h)))
}

#[cfg(any(test, windows))] // Windows only; tested everywhere.
/// What Windows' placement record (`GetWindowPlacement`) says about a window, reduced to
/// what a save needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Placement {
    /// Minimized now (`showCmd` is `SW_SHOWMINIMIZED`).
    pub minimized: bool,
    /// `WPF_RESTORETOMAXIMIZED`: restoring from minimized returns to maximized. Windows
    /// leaves the flag set after such a window is restored and then un-maximized, so it
    /// means something only while `minimized`.
    pub restores_maximized: bool,
    /// The restore rectangle (`rcNormalPosition`) as a logical client size, or `None`
    /// when it could not be converted.
    pub normal_size: Option<(i32, i32)>,
}

#[cfg(any(test, windows))] // Windows only; tested everywhere.
/// The size and maximized state to save for a window GTK reads as `size` and
/// `maximized`, given its placement record.
///
/// Minimizing a maximized window clears `WS_MAXIMIZE`, so GTK reads it as not maximized,
/// while its default size is still the maximized size (`win32.rs`'s header). Saved as
/// read, it would come back as a plain window the size of the screen. The placement
/// record still holds both facts: the flag says it returns to maximized, and the
/// restore rectangle is the size it had before. A maximized or minimized window takes its
/// size from that rectangle; any other keeps GTK's reading.
pub(crate) fn saved_state(
    size: (i32, i32),
    maximized: bool,
    placement: Option<Placement>,
) -> ((i32, i32), bool) {
    let Some(placement) = placement else {
        return (size, maximized);
    };
    let maximized = maximized || (placement.minimized && placement.restores_maximized);
    let size = if maximized || placement.minimized {
        placement.normal_size.unwrap_or(size)
    } else {
        size
    };
    (size, maximized)
}

#[cfg(any(test, all(unix, not(target_os = "macos"))))] // X11 only; tested everywhere.
/// The nil UUID, which KWin reads as "every activity".
pub(crate) const ALL_ACTIVITIES: &str = "00000000-0000-0000-0000-000000000000";

#[cfg(any(test, all(unix, not(target_os = "macos"))))] // X11 only; tested everywhere.
/// The activities to write for a restored window: the saved ones that still exist.
///
/// `running` is the activity manager's current list, or `None` when it could not be
/// asked. Unasked, the saved list is written as it is and KWin drops the ones it does
/// not know. The nil UUID is always kept. An empty result means "write nothing", so the
/// window lands on the current activity like a new window; KWin, given a list that
/// matches nothing, would put it on every activity instead.
pub(crate) fn surviving_activities(saved: &[String], running: Option<&[String]>) -> Vec<String> {
    saved
        .iter()
        .filter(|a| !a.is_empty())
        .filter(|a| {
            a.as_str() == ALL_ACTIVITIES
                || running.is_none_or(|r| r.iter().any(|known| known == *a))
        })
        .cloned()
        .collect()
}

#[cfg(any(test, all(unix, not(target_os = "macos"))))] // X11 only; tested everywhere.
/// The activities as KWin's property spells them: comma-separated UUIDs.
pub(crate) fn split_activities(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MON: Rect = Rect {
        x: 1920,
        y: 0,
        width: 2560,
        height: 1440,
    };

    #[test]
    fn a_position_with_the_title_bar_on_screen_is_kept_exactly() {
        assert_eq!(frame_origin(MON, (100, 200), 800), (2020, 200));
        // Hanging off the right and left edges, title bar still reachable.
        assert_eq!(frame_origin(MON, (2400, 10), 800), (4320, 10));
        assert_eq!(frame_origin(MON, (-500, 10), 800), (1420, 10));
    }

    #[test]
    fn a_monitor_that_shrank_pulls_the_window_back_until_its_title_bar_is_reachable() {
        let small = Rect {
            width: 1280,
            height: 720,
            ..MON
        };
        // Saved at the far corner of the big monitor.
        assert_eq!(
            frame_origin(small, (2400, 1300), 800),
            (1920 + 1280 - KEEP_REACHABLE, 720 - KEEP_REACHABLE)
        );
        // Above the top edge, where the title bar is.
        assert_eq!(frame_origin(small, (10, -40), 800), (1930, 0));
        // Further off the left than the window is wide.
        assert_eq!(
            frame_origin(small, (-5000, 10), 800),
            (1920 - (800 - KEEP_REACHABLE), 10)
        );
    }

    #[test]
    fn a_window_narrower_than_the_keep_margin_stays_wholly_on_screen() {
        assert_eq!(frame_origin(MON, (-100, 0), 20), (1920, 0));
    }

    #[test]
    fn size_is_fitted_to_the_saved_monitor_when_it_is_still_listed() {
        let laptop = Rect {
            x: 0,
            y: 0,
            width: 1366,
            height: 768,
        };
        assert_eq!(fit_size((3000, 2000), Some(MON), &[laptop]), (2560, 1440));
        assert_eq!(fit_size((800, 600), Some(MON), &[laptop]), (800, 600));
        // The saved monitor wins over a bigger one listed beside it.
        assert_eq!(fit_size((2000, 1000), Some(laptop), &[MON]), (1366, 768));
    }

    #[test]
    fn a_saved_monitor_that_is_gone_fits_the_size_to_the_largest_listed() {
        let laptop = Rect {
            x: 0,
            y: 0,
            width: 1366,
            height: 768,
        };
        let portrait = Rect {
            x: 1366,
            y: 0,
            width: 1080,
            height: 1920,
        };
        // Saved at 2560 px on an external screen, restored on the laptop alone.
        assert_eq!(fit_size((2560, 1400), None, &[laptop]), (1366, 768));
        // Each dimension against the largest any monitor here offers.
        assert_eq!(
            fit_size((2560, 1400), None, &[laptop, portrait]),
            (1366, 1400)
        );
        assert_eq!(fit_size((800, 600), None, &[laptop]), (800, 600));
        // A monitor with no area is not one to fit to, as a saved one or a listed one.
        let empty = Rect { width: 0, ..laptop };
        assert_eq!(fit_size((2560, 1400), Some(empty), &[laptop]), (1366, 768));
        assert_eq!(fit_size((3000, 2000), None, &[empty]), (3000, 2000));
        assert_eq!(fit_size((3000, 2000), None, &[]), (3000, 2000));
    }

    #[test]
    fn the_scale_is_measured_from_the_surface_so_a_fractional_one_is_kept() {
        // 150 %: a 2880 px client area GDK calls 1920 logical pixels.
        assert_eq!(device_scale(2880, 1920, 2), 1.5);
        assert_eq!(device_scale(2880, 1440, 2), 2.0);
        // Unknown widths fall back to the integer factor, never below 1.
        assert_eq!(device_scale(0, 1920, 2), 2.0);
        assert_eq!(device_scale(2880, 0, 0), 1.0);
    }

    #[test]
    fn a_physical_size_is_divided_by_the_scale_and_rounded() {
        assert_eq!(to_logical((1500, 900), 1.5), Some((1000, 600)));
        assert_eq!(to_logical((1501, 899), 1.25), Some((1201, 719)));
        assert_eq!(to_logical((1600, 1000), 2.0), Some((800, 500)));
        assert_eq!(to_logical((0, 900), 1.5), None);
        assert_eq!(to_logical((1500, 900), 0.0), None);
    }

    /// MEASURED on Windows 10, GTK 4.22.4, at 100%: a window of client 935x538, maximized
    /// and then minimized, reads to GTK as not maximized at the maximized client size
    /// 2025x1141, while its placement says minimized, `WPF_RESTORETOMAXIMIZED`, and the
    /// restore rectangle is 935x538.
    #[test]
    fn a_maximized_window_minimized_at_quit_is_saved_maximized_at_its_own_size() {
        let placement = Placement {
            minimized: true,
            restores_maximized: true,
            normal_size: Some((935, 538)),
        };
        assert_eq!(
            saved_state((2025, 1141), false, Some(placement)),
            ((935, 538), true)
        );
    }

    #[test]
    fn a_plain_window_minimized_is_saved_plain_at_its_restore_size() {
        let placement = Placement {
            minimized: true,
            restores_maximized: false,
            normal_size: Some((861, 494)),
        };
        assert_eq!(
            saved_state((861, 494), false, Some(placement)),
            ((861, 494), false)
        );
    }

    #[test]
    fn a_maximized_window_takes_its_size_from_the_restore_rectangle() {
        let placement = Placement {
            minimized: false,
            restores_maximized: true,
            normal_size: Some((985, 574)),
        };
        assert_eq!(
            saved_state((2025, 1141), true, Some(placement)),
            ((985, 574), true)
        );
    }

    /// Windows leaves `WPF_RESTORETOMAXIMIZED` set on a window restored from minimized to
    /// maximized and then to normal (MEASURED as above), so the flag alone must not make a
    /// plain window maximized.
    #[test]
    fn a_stale_restore_to_maximized_flag_on_a_plain_window_is_ignored() {
        let placement = Placement {
            minimized: false,
            restores_maximized: true,
            normal_size: Some((935, 538)),
        };
        assert_eq!(
            saved_state((935, 538), false, Some(placement)),
            ((935, 538), false)
        );
    }

    #[test]
    fn without_a_placement_or_its_size_gtk_s_reading_stands() {
        assert_eq!(saved_state((800, 600), true, None), ((800, 600), true));
        let no_size = Placement {
            minimized: true,
            restores_maximized: true,
            normal_size: None,
        };
        assert_eq!(
            saved_state((2025, 1141), false, Some(no_size)),
            ((2025, 1141), true)
        );
    }

    #[test]
    fn an_activity_that_no_longer_exists_is_dropped_and_the_rest_kept() {
        let saved = vec!["a".to_owned(), "gone".to_owned(), "b".to_owned()];
        let running = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
        assert_eq!(
            surviving_activities(&saved, Some(&running)),
            vec!["a".to_owned(), "b".to_owned()]
        );
    }

    #[test]
    fn when_none_survive_nothing_is_written() {
        let saved = vec!["gone".to_owned()];
        assert!(surviving_activities(&saved, Some(&["a".to_owned()])).is_empty());
    }

    #[test]
    fn all_activities_survives_and_an_unreachable_manager_keeps_the_list() {
        let saved = vec![ALL_ACTIVITIES.to_owned()];
        assert_eq!(surviving_activities(&saved, Some(&[])), saved);
        let saved = vec!["x".to_owned(), "y".to_owned()];
        assert_eq!(surviving_activities(&saved, None), saved);
    }

    #[test]
    fn the_property_splits_on_commas_and_skips_empties() {
        assert_eq!(split_activities("a,b"), vec!["a", "b"]);
        assert!(split_activities("").is_empty());
        assert_eq!(split_activities("a,,b,"), vec!["a", "b"]);
    }
}
