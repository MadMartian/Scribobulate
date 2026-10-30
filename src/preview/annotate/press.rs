//! The two decisions the preview's cell-press controller makes about a raw event, out of
//! the GTK wiring so they test without a display.
//!
//! The controller (`overlay::wire_cell_press_exclusivity`) is an `EventControllerLegacy`,
//! and gtk-rs cannot build the `gdk::Event` such a controller is handed, so a test cannot
//! drive it with a press. What it decides — whether the event is a press it acts on, and
//! where that press happened — is plain arithmetic over the event's fields, and lives here.

use gtk::gdk;

/// Whether a raw event is one the one-selection rule acts on: a press of the primary
/// button. Any other event, and any other button, is left alone, so a right-click on a
/// selection keeps it for the context menu (TDD 17.63). `button` is `None` for an event
/// that carries no button.
pub(super) fn is_primary_press(event_type: gdk::EventType, button: Option<u32>) -> bool {
    event_type == gdk::EventType::ButtonPress && button == Some(gdk::BUTTON_PRIMARY)
}

/// A raw event's surface coordinates in its native widget's own coordinates.
///
/// A surface is larger than the native widget it holds by the native's surface transform
/// (a client-side decoration's shadow, for one), so the native's origin sits at
/// `transform` in the surface, and the point moves the other way.
pub(super) fn surface_to_native(surface: (f64, f64), transform: (f64, f64)) -> (f64, f64) {
    let (sx, sy) = surface;
    let (tx, ty) = transform;
    (sx - tx, sy - ty)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TDD 17.63: only a primary-button press is acted on.
    #[test]
    fn only_a_primary_button_press_is_acted_on() {
        use gdk::EventType::{ButtonPress, ButtonRelease, MotionNotify};
        assert!(is_primary_press(ButtonPress, Some(gdk::BUTTON_PRIMARY)));
        assert!(!is_primary_press(ButtonPress, Some(gdk::BUTTON_SECONDARY)));
        assert!(!is_primary_press(ButtonPress, Some(gdk::BUTTON_MIDDLE)));
        assert!(!is_primary_press(ButtonPress, None));
        assert!(!is_primary_press(ButtonRelease, Some(gdk::BUTTON_PRIMARY)));
        assert!(!is_primary_press(MotionNotify, None));
    }

    /// The native widget's origin is at the surface transform, so a press at the
    /// transform is at the native's origin, and the offset is subtracted on both axes.
    #[test]
    fn a_surface_point_moves_back_by_the_surface_transform() {
        assert_eq!(surface_to_native((30.0, 40.0), (30.0, 40.0)), (0.0, 0.0));
        assert_eq!(
            surface_to_native((130.0, 75.5), (26.0, 23.0)),
            (104.0, 52.5)
        );
        assert_eq!(surface_to_native((10.0, 20.0), (0.0, 0.0)), (10.0, 20.0));
    }
}
