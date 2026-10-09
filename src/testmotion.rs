//! Test-only. Pins GTK's reduced-motion setting OFF for a test whose subject is a
//! scroll ANIMATION, so that test exercises the animated path on every platform.
//!
//! **Why this exists.** From GTK 4.24, `gtk_adjustment_animate_to_value` reads
//! `GtkSettings:gtk-interface-reduced-motion` and, when it says `reduce`, jumps
//! instead of animating (`gtkadjustment.c`, "adjustment: Respect the reduced-motion
//! setting"; GTK4Rs/AP-357). Every `GtkTextView` scroll — `scroll_to_mark`, the
//! buffer-ends key bindings — goes through it. Several tests here guard a defect that
//! only exists BECAUSE the scroll animates: GTK's own scroll dies on the first chunk of
//! line-height validation, or a write truncates it mid-flight (GTK4Rs/AP-260). With
//! reduced motion on there is no animation to die, GTK's own scroll arrives, and those
//! tests pass with the code they guard deleted. GitHub's hosted macOS runners have
//! Reduce Motion on, so that is not hypothetical.
//!
//! **Why the pin holds.** A value set on `GtkSettings` by the application is recorded
//! with source `GTK_SETTINGS_SOURCE_APPLICATION`, and a desktop change arriving from the
//! backend (on macOS, `NSWorkspace accessibilityDisplayShouldReduceMotion` re-read by
//! `_gdk_macos_display_reload_settings`) is refused for such a property:
//! `settings_update_xsetting` returns early on an application source (`gtksettings.c`
//! 4.24.1). [`AnimatedScrolls::pin`] reads the value back to prove it took.
//!
//! **Why it is restored with `reset_property`, not by writing the old value back.**
//! Writing the old value would leave the property application-sourced, i.e. frozen
//! against the desktop for every later test in the process. `gtk_settings_reset_property`
//! drops the application source, and the next read follows the desktop again
//! (`gtk_settings_get_property` re-reads a non-application value from the display).
//!
//! **Where the pin cannot apply** it says so on stderr rather than doing nothing
//! silently: GTK before 4.22 has no such property. Below 4.24 the adjustment does not
//! consult it at all, so on those versions the animated path is the only path and the
//! test is already exercising it.

use gtk::glib;
use gtk::prelude::*;

/// The `GtkSettings` property `gtk_adjustment_animate_to_value` consults (GTK ≥ 4.24).
/// Read by name at runtime: the crate targets GTK 4.6, whose bindings have no accessor.
const REDUCED_MOTION: &str = "gtk-interface-reduced-motion";

/// The `GtkReducedMotion` nick that lets an adjustment animate.
const NO_PREFERENCE: &str = "no-preference";

/// While alive, GTK scroll animations animate whatever the desktop's reduced-motion
/// preference is. Take it as the first line of the test body and keep it bound
/// (`let _motion = …`) until the body ends.
#[must_use = "the pin lasts only as long as the guard is alive"]
pub(crate) struct AnimatedScrolls {
    /// `None` where this GTK has no reduced-motion setting, so there is nothing to undo.
    pinned: Option<gtk::Settings>,
}

impl AnimatedScrolls {
    /// Pin reduced motion OFF on the default `GtkSettings` — the instance
    /// `gtk_adjustment_animate_to_value` reads.
    pub(crate) fn pin() -> Self {
        let settings = gtk::Settings::default().expect("a display exists under gtktest");
        // The scrolled window's own gate on animating at all: it hands its adjustments a
        // frame clock and a duration only while this is true (gtkscrolledwindow.c,
        // `gtk_widget_should_animate`). Not fed by the macOS or Windows backends, so it is
        // only ever false here because a test left it so; say that rather than letting a
        // pinned test pass on a path it did not exercise.
        assert!(
            settings.is_gtk_enable_animations(),
            "gtk-enable-animations is off, so scrolls jump whatever reduced motion says \
             and this test cannot exercise the animated path — something earlier in the \
             process left it off"
        );
        let Some(pspec) = settings.find_property(REDUCED_MOTION) else {
            eprintln!(
                "NOTE [reduced-motion pin]: GTK {}.{}.{} has no `{REDUCED_MOTION}` setting \
                 (added in 4.22), so it is not pinned. Scroll animations here do not consult \
                 reduced motion, so this run exercises the animated path regardless.",
                gtk::major_version(),
                gtk::minor_version(),
                gtk::micro_version(),
            );
            return Self { pinned: None };
        };
        let class = glib::EnumClass::with_type(pspec.value_type())
            .expect("gtk-interface-reduced-motion is an enum property");
        let before = nick_of(&settings);
        let off = class
            .to_value_by_nick(NO_PREFERENCE)
            .expect("GtkReducedMotion has a `no-preference` value");
        settings.set_property_from_value(REDUCED_MOTION, &off);
        let after = nick_of(&settings);
        assert_eq!(
            after, NO_PREFERENCE,
            "pinning `{REDUCED_MOTION}` did not take: it reads `{after}`"
        );
        if before != NO_PREFERENCE {
            eprintln!(
                "NOTE [reduced-motion pin]: the desktop asks for `{before}`; pinned to \
                 `{NO_PREFERENCE}` for this test so its scroll animates."
            );
        }
        Self {
            pinned: Some(settings),
        }
    }
}

impl Drop for AnimatedScrolls {
    fn drop(&mut self) {
        if let Some(settings) = &self.pinned {
            settings.reset_property(REDUCED_MOTION);
        }
    }
}

/// The current `gtk-interface-reduced-motion` value's nick.
fn nick_of(settings: &gtk::Settings) -> String {
    let value = settings.property_value(REDUCED_MOTION);
    glib::EnumValue::from_value(&value)
        .map(|(_, v)| v.nick().to_owned())
        .expect("gtk-interface-reduced-motion holds an enum value")
}
