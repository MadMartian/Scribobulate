//! macOS light/dark appearance tracking — the source GTK never supplies itself.
//!
//! **GTK does not read the macOS appearance, at all.** Verified against the
//! shipped GTK 4.22.4: `libgtk-4.1.dylib` contains zero occurrences of
//! `AppleInterfaceStyle`, `effectiveAppearance` or `DarkAqua`, so nothing in the
//! toolkit ever consults the setting on this backend. Measured to match: with
//! macOS already in dark mode at launch, `gtk-application-prefer-dark-theme` is
//! `FALSE` and `theme_base_color` is white; on a live flip, in *either*
//! direction, GTK still holds the stale value until something writes it. That is
//! why the gap this closes was bigger than the KDE/X11 one — there a fresh launch
//! at least picks the new scheme up, here neither path did.
//!
//! Like `platform/win32/appearance.rs` (the Windows port's precedent for this exact gap, same
//! cause, different OS), this module deliberately **writes into
//! `gtk-application-prefer-dark-theme` and re-themes nothing itself**. That
//! property is already the app's live-theme channel: `app::setup::connect_theme_change`
//! subscribes to it and re-renders every window, every tab, the editor scheme and
//! the generated theme sheet. So this supplies the missing *source* for a path
//! that already exists rather than adding a parallel one. Re-theming a surface
//! directly from a platform signal would build a second channel that drifts from
//! the first; the desktop's lightness keeps one answer, `palette::desktop_is_dark()`.
//! Measured downstream: writing the property moves `theme_base_color` from
//! `lum=1.000` to `lum=0.026`, so `desktop_is_dark()` follows, the palette
//! resolves to a dark page (`page_bg=#2d2d2d`, `body_fg=#ffffff`) and
//! `re_render_all_windows` runs with `dark=true` — and, with the colour scheme
//! below also set, the window actually paints dark.
//!
//! **The 4.20 twist, which cost most of the effort here.** Writing only
//! `gtk-application-prefer-dark-theme` moves the palette and re-renders the app
//! while leaving the window *painted light*, and every intermediate check still
//! reads correct — see ScrAP-184. GTK 4.20 deprecated that property and made the
//! default theme's `gtk.css` a set of `@media (prefers-color-scheme: …)` imports
//! driven by `gtk-interface-color-scheme`, in which `UNSUPPORTED`, `DEFAULT` and
//! `LIGHT` all evaluate as **light** — and the Quartz backend never sets it. So
//! both properties are written, and the painted result is asserted on pixels
//! rather than inferred: `tests/macos_dark_mode.rs` renders a real window through
//! GSK and samples it (`rgb(53,53,53)` dark, `rgb(246,245,244)` light).
//!
//! That rule now lives in `crate::colorscheme` rather than in this file, because it
//! is a GTK-*version* rule and `platform/win32/appearance.rs` needs the same copy of it: Windows
//! shipped writing only the legacy half for exactly as long as this module owned
//! it (QA round R1-04).
//!
//! **Why CoreFoundation and not KVO on `NSApp.effectiveAppearance`.** KVO is the
//! documented Cocoa channel and was the researched recommendation, but observing
//! a key from Rust means *synthesising an Objective-C class at runtime*
//! (`objc_allocateClassPair` + `class_addMethod` for
//! `observeValueForKeyPath:ofObject:change:context:`) or taking an `objc2`
//! dependency. `CFNotificationCenterAddObserver` takes a plain C function
//! pointer, so the same signal is reachable with public CoreFoundation C API and
//! no Objective-C runtime work at all — which keeps the "hand-rolled FFI, no new
//! binding crate" rule its neighbour `single_instance.rs` and `platform/win32/` both
//! follow. Measured working in both directions, and at launch.
//!
//! The one caveat worth stating plainly: `AppleInterfaceThemeChangedNotification`
//! is a *de facto* name (widely relied on — Chromium, Qt and Electron all use it)
//! rather than one published in a header. Its failure mode is benign and bounded:
//! if a future macOS stopped posting it, the launch-time read below still works
//! and the app would simply be back to needing a restart, never wrong or crashed.

use gtk::glib;
use std::ffi::c_void;

// ── CoreFoundation FFI ───────────────────────────────────────────────────────
//
// Declared by hand rather than via `core-foundation`/`objc2`: six functions and
// one static cost less than a crate, and every one of them is public, documented
// C API — no vtable reach, nothing private. Same bargain `platform/win32/` struck
// for `dwmapi`.

type CFTypeRef = *const c_void;
type CFStringRef = *const c_void;
type CFNotificationCenterRef = *const c_void;
type CFIndex = isize;
type CFTypeID = usize;
type Boolean = u8;

/// `kCFStringEncodingUTF8`.
const UTF8: u32 = 0x0800_0100;
/// `CFNotificationSuspensionBehaviorDeliverImmediately`. The app is a normal
/// foreground GUI process, so it is never suspended in the sense the other
/// behaviours coalesce for; delivering immediately keeps a live flip live.
const DELIVER_IMMEDIATELY: CFIndex = 4;

/// The de-facto distributed-notification name macOS posts on an appearance flip.
/// Also fires when the "Auto" appearance switches at sunrise/sunset, which is why
/// the handler re-reads the setting rather than toggling a remembered value.
const APPEARANCE_CHANGED: &str = "AppleInterfaceThemeChangedNotification";

/// The global-domain preference holding the *effective* appearance. Absent means
/// light — macOS does not write `"Light"`, it removes the key.
const INTERFACE_STYLE_KEY: &str = "AppleInterfaceStyle";

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFPreferencesAnyApplication: CFStringRef;

    fn CFRelease(cf: CFTypeRef);
    fn CFGetTypeID(cf: CFTypeRef) -> CFTypeID;
    fn CFStringGetTypeID() -> CFTypeID;
    fn CFStringCreateWithBytes(
        alloc: CFTypeRef,
        bytes: *const u8,
        num_bytes: CFIndex,
        encoding: u32,
        is_external_representation: Boolean,
    ) -> CFStringRef;
    fn CFStringGetCString(
        the_string: CFStringRef,
        buffer: *mut u8,
        buffer_size: CFIndex,
        encoding: u32,
    ) -> Boolean;
    fn CFPreferencesCopyAppValue(key: CFStringRef, application_id: CFStringRef) -> CFTypeRef;
    fn CFPreferencesAppSynchronize(application_id: CFStringRef) -> Boolean;
    fn CFNotificationCenterGetDistributedCenter() -> CFNotificationCenterRef;
    fn CFNotificationCenterAddObserver(
        center: CFNotificationCenterRef,
        observer: *const c_void,
        callback: NotificationCallback,
        name: CFStringRef,
        object: *const c_void,
        suspension_behavior: CFIndex,
    );
}

type NotificationCallback = extern "C" fn(
    center: CFNotificationCenterRef,
    observer: *mut c_void,
    name: CFStringRef,
    object: *const c_void,
    user_info: *const c_void,
);

/// An owned `CFStringRef`, released on drop. Only ever built from a Rust `&str`,
/// so the bytes are valid UTF-8 by construction.
struct CfString(CFStringRef);

impl CfString {
    fn new(s: &str) -> Option<Self> {
        // SAFETY: `s` is a live Rust string for the duration of the call and its
        // length is its true byte length. A null allocator means the default one.
        let raw = unsafe {
            CFStringCreateWithBytes(
                std::ptr::null(),
                s.as_ptr(),
                s.len() as CFIndex,
                UTF8,
                0, // not an external representation: no BOM handling
            )
        };
        (!raw.is_null()).then_some(Self(raw))
    }
}

impl Drop for CfString {
    fn drop(&mut self) {
        // SAFETY: `self.0` came from a Create function, so we own exactly one
        // reference, and it is non-null by the constructor's check.
        unsafe { CFRelease(self.0) };
    }
}

// ── reading the setting ──────────────────────────────────────────────────────

/// Map the raw `AppleInterfaceStyle` value to "is dark".
///
/// Split out as a pure function because it is the one piece here that can be
/// tested without a display **or** a live appearance flip, and because getting it
/// backwards is both easy and near-undetectable: an inverted mapping still looks
/// like working dark-mode support, just on the wrong days.
///
/// `None` (the key is absent) is light — that is macOS's actual representation of
/// light mode, not a missing-value case to be cautious about. Anything else
/// unrecognised is also treated as light, which is the pre-existing behaviour and
/// therefore the safe direction to fail in.
fn style_is_dark(value: Option<&str>) -> bool {
    matches!(value, Some(v) if v.eq_ignore_ascii_case("Dark"))
}

/// Copy a `CFTypeRef` out as a Rust `String`, or `None` if it is not a CFString.
///
/// The type check is not ceremony. `AppleInterfaceStyle` lives in the *global*
/// preferences domain, which the user can write anything into
/// (`defaults write -g AppleInterfaceStyle -int 5`), and calling a CFString
/// function on a CFNumber is undefined behaviour, not an error return.
///
/// # Safety
/// `value` must be a valid, non-null CF object the caller still owns.
unsafe fn cf_string_to_owned(value: CFTypeRef) -> Option<String> {
    // SAFETY: caller guarantees `value` is a live CF object.
    if unsafe { CFGetTypeID(value) } != unsafe { CFStringGetTypeID() } {
        return None;
    }
    // "Dark" is four bytes; this is sized so that anything plausible fits and
    // anything absurd simply fails the conversion (and reads as light).
    let mut buf = [0u8; 64];
    // SAFETY: `value` is a CFString per the check above; the buffer and its
    // stated size agree, and CFStringGetCString NUL-terminates within them.
    let ok = unsafe { CFStringGetCString(value, buf.as_mut_ptr(), buf.len() as CFIndex, UTF8) };
    if ok == 0 {
        return None;
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    std::str::from_utf8(&buf[..end]).ok().map(str::to_owned)
}

/// Whether macOS is currently in dark mode.
///
/// Reads the *effective* style, so the "Auto" appearance is handled for free: when
/// it switches at sunset macOS sets this key exactly as an explicit choice would.
pub(crate) fn macos_prefers_dark() -> bool {
    // Belt: CFPreferences may serve a cached copy of a domain. Measured on
    // 4.22.4/macOS here, the unsynchronized read already returned the NEW value
    // at notification time in both directions — this call was not needed to make
    // it work. It is kept because the cost is one call per *appearance flip*
    // (not per poll — nothing polls here), and the alternative failure is a
    // window left in the wrong theme until the user flips again.
    // SAFETY: the argument is CoreFoundation's own global-domain constant.
    unsafe { CFPreferencesAppSynchronize(kCFPreferencesAnyApplication) };

    let Some(key) = CfString::new(INTERFACE_STYLE_KEY) else {
        return false;
    };
    // SAFETY: `key` is live for the call; the application ID is CF's own static.
    let value = unsafe { CFPreferencesCopyAppValue(key.0, kCFPreferencesAnyApplication) };

    // A null value — the key is absent, i.e. light mode — is handed to
    // `style_is_dark` as `None` rather than short-circuited to `false` here.
    //
    // That is deliberate, and it was a mutation test that forced it. An earlier
    // version returned `false` directly on null, which put "absent means light"
    // in *two* places and made the whole mapping unreachable on a light machine:
    // inverting `style_is_dark` left the live cross-check below still passing,
    // because it never ran. One expression of the rule, always exercised, means
    // the polarity assertions have teeth in whichever state the machine is in.
    let style = if value.is_null() {
        None
    } else {
        // SAFETY: non-null per the check above, and `Copy` transferred ownership
        // to us, so it stays alive until the `CFRelease` below.
        let style = unsafe { cf_string_to_owned(value) };
        // SAFETY: we own the one reference `CopyAppValue` handed over.
        unsafe { CFRelease(value) };
        style
    };
    style_is_dark(style.as_deref())
}

// ── writing it into GTK ──────────────────────────────────────────────────────

/// Hand the appearance to the shared writer, which owns **which** GTK settings
/// carry it and in what order.
///
/// That rule is a GTK-version rule rather than a macOS one — writing only the
/// legacy `gtk-application-prefer-dark-theme` leaves a 4.20+ window painted light
/// while every intermediate reading looks correct — so it lives in
/// `crate::colorscheme`, where `platform/win32/appearance.rs` reads the same copy. It used to live
/// here, and Windows duly shipped with only half of it (QA round R1-04).
fn apply(dark: bool) {
    crate::colorscheme::apply_desktop_lightness(dark);
}

/// CoreFoundation's callback: reads the appearance **now** and applies it **now**.
///
/// ⚠ **Do not "improve" this by deferring the work to `glib::idle_add_once`.** The
/// first version did exactly that — the textbook marshal-to-the-main-thread move —
/// and it did not work. Measured, on a real flip with the app otherwise idle:
///
/// ```text
/// callback FIRED (main=1), os_dark=true     <- the flip to DARK
/// ... 8 seconds, no dispatch ...
/// idle ran, os_dark=false                   <- reads the appearance AFTER the flip BACK
/// ```
///
/// An idle queued from a CoreFoundation callback is not dispatched until
/// something else iterates the GLib main context, and on the Quartz backend an
/// idle app iterates it rarely — adding an unrelated one-second `timeout_add`
/// made the same code work, which is what isolated it. So the deferral is not
/// merely *late*: the deferred read observes whatever the appearance happens to
/// be when the loop finally wakes, which for a there-and-back flip is the
/// opposite of the event that triggered it. **Read at the event; never at the
/// convenience of the loop.**
///
/// That leaves the two hazards the deferral was meant to cover, handled directly:
///
/// * **Panics.** A Rust panic may not unwind across an `extern "C"` boundary; it
///   is converted into an immediate `abort`, i.e. SIGABRT and a macOS crash
///   report rather than an error. `catch_unwind` keeps the boundary sealed.
/// * **Threads.** GTK on macOS may only be touched from the process main thread.
///   Measured, this notification *is* delivered on the main thread
///   (`pthread_main_np() == 1`) — GDK's macOS event source already pumps the run
///   loop that carries it. The off-main-thread branch is therefore unreached
///   today, and it still carries the *value* rather than re-reading later, so it
///   cannot reintroduce the staleness above.
extern "C" fn on_appearance_changed(
    _center: CFNotificationCenterRef,
    _observer: *mut c_void,
    _name: CFStringRef,
    _object: *const c_void,
    _user_info: *const c_void,
) {
    // A panic escaping this frame would abort the process, so nothing inside it
    // is allowed to reach the `extern "C"` boundary.
    let _ = std::panic::catch_unwind(|| {
        let dark = macos_prefers_dark();
        // SAFETY: a thread-identity query with no arguments and no side effects.
        if unsafe { libc::pthread_main_np() } != 0 {
            apply(dark);
        } else {
            glib::idle_add_once(move || apply(dark));
        }
    });
}

/// Make the app follow the macOS light/dark appearance, now and whenever the user
/// changes it. Call once, from `startup`, **before** the first window is built so
/// it opens in the right theme rather than flashing light and re-rendering.
///
/// Named to match `platform::win32::track_system_dark_mode` on purpose: it is the same job,
/// against the same GTK gap, and the symmetry is the point.
pub(crate) fn track_system_dark_mode() {
    apply(macos_prefers_dark());

    let Some(name) = CfString::new(APPEARANCE_CHANGED) else {
        log::warn!("macOS appearance: cannot build the notification name; live tracking disabled");
        return;
    };

    // SAFETY: the centre is a process-wide singleton, the callback is a plain
    // `extern "C"` function with the signature CF expects, and `name` is a live
    // CFString for the duration of the call. A null observer/object means "no
    // context, any sender", which is what a system-wide appearance
    // notification has. Deliberately makes no claim about whether CF *retains*
    // `name` — see the `std::mem::forget` below, which is where that question is
    // dealt with rather than asserted away.
    unsafe {
        CFNotificationCenterAddObserver(
            CFNotificationCenterGetDistributedCenter(),
            std::ptr::null(),
            on_appearance_changed,
            name.0,
            std::ptr::null(),
            DELIVER_IMMEDIATELY,
        );
    }
    // QA finding R1-S03: whether `CFNotificationCenterAddObserver` retains
    // `name` is not settled. It is not documented either way, and — unlike
    // `CFUserNotification.c`, which sits right next to it in the same
    // framework — `CFNotificationCenter.c` has never appeared in any of
    // Apple's open-source CF drops (checked `opensource-apple/CF` and its
    // successor `apple-oss-distributions/CF`; both ship the former, neither
    // the latter), so the question cannot be settled by reading source either.
    // Rather than leave that open, sidestep it: the observer below is never
    // removed for the life of the process anyway (see the comment there), so
    // deliberately leaking this one small CFStringRef to match — `forget`
    // skips `CfString`'s `Drop` (which would `CFRelease` it) — costs nothing
    // and removes the ambiguity by construction, whichever way CF's actual
    // contract runs.
    std::mem::forget(name);
    // Never removed: the observer lives as long as the process, and there is no
    // teardown point at which removing it would be more correct than letting the
    // process exit.
    log::debug!("macOS appearance: tracking {APPEARANCE_CHANGED}");
}

#[cfg(test)]
mod tests {
    // Path-qualified rather than a `use super::*` glob, which would be reported
    // unused whenever this file is compiled into the `harness = false` target in
    // `tests/macos_dark_mode.rs`: cargo passes `--cfg test` there but not
    // `--test`, so this module is compiled while the `#[test]` bodies inside it
    // are stripped, leaving the import with nothing to serve.

    /// Pins the POLARITY, which is the one thing here that is easy to get exactly
    /// backwards and impossible to notice in review — an inverted mapping is
    /// still "working dark mode", just always wrong.
    ///
    /// Note what macOS actually stores: there is no `"Light"` value. Light mode is
    /// the *absence* of the key, which is why `None` is a real, expected input
    /// here rather than an error case.
    #[test]
    fn dark_is_the_only_dark() {
        assert!(super::style_is_dark(Some("Dark")));
        assert!(
            super::style_is_dark(Some("dark")),
            "matched case-insensitively"
        );
        assert!(
            !super::style_is_dark(None),
            "absent means LIGHT, not unknown"
        );
        assert!(!super::style_is_dark(Some("Light")));
        assert!(!super::style_is_dark(Some("")));
        assert!(!super::style_is_dark(Some("Darkish")));
    }

    /// Cross-checks the live CF read against an independent source — the `defaults`
    /// tool — so the FFI path is pinned to something that was not written by this
    /// module. A unit test can do this because reading the preference touches no
    /// GTK at all; anything that did could not be a unit test here, since libtest
    /// runs bodies on worker threads and GTK on macOS refuses those.
    ///
    /// This confirms whichever direction the machine happens to be in. That is a
    /// real limit, and the reason the pure mapping above is tested separately
    /// rather than relying on this.
    #[test]
    fn live_read_agrees_with_defaults() {
        let out = std::process::Command::new("defaults")
            .args(["read", "-g", "AppleInterfaceStyle"])
            .output()
            .expect("`defaults` is present on macOS");
        // A non-zero exit means the key is absent, i.e. light mode.
        let expected = out.status.success()
            && String::from_utf8_lossy(&out.stdout)
                .trim()
                .eq_ignore_ascii_case("Dark");
        assert_eq!(
            super::macos_prefers_dark(),
            expected,
            "the CoreFoundation read disagrees with `defaults read -g AppleInterfaceStyle`"
        );
    }
}
