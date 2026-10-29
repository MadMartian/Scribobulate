//! Per-platform seams, one module per target OS — a directory where the platform
//! needs more than one file, a single file where it does not.
//!
//! Each child is `#[cfg]`-gated **at its declaration** here and never internally,
//! so a build for another platform compiles none of it and the `-D warnings`
//! clippy gate stays satisfiable with no `#[allow]` (the same rule `workaround`
//! follows).
//!
//! Declaring them here rather than at the crate root is what makes that gating a
//! single fact: `lib.rs` and `gtk_suite.rs` are two crate roots that must otherwise
//! re-declare every module, and a platform module missing from the second silently
//! drops its whole test surface from the main-thread suite.
//!
//! The bar for putting something here is narrow: it must be work the toolkit does
//! for us elsewhere and does not do on this platform. Anything that changes what
//! the app *does* belongs in the shared code, behind a capability, so the
//! platforms cannot drift in behaviour — only in the plumbing underneath.

#[cfg(target_os = "macos")]
pub(crate) mod mac;

#[cfg(windows)]
pub(crate) mod win32;

#[cfg(target_os = "linux")]
pub(crate) mod linux;

// X11 is a display backend rather than an OS, but it is gated here for the same reason:
// a platform whose GTK has no X11 backend must compile none of it.
#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) mod x11;

// ── where a point on a surface is on the screen ─────────────────────────────────────

/// Where `(x, y)` on `surface` lies in screen coordinates — the space `gdk::Monitor`
/// geometry is in — or `None` where this app has no use for the answer.
///
/// Only X11 answers. GTK 4.6–4.12's X11 popup layout resolves a popover's monitor from
/// its anchor in these coordinates with no fallback (GTK4Rs/AP-26), so
/// `saferizer::popover_anchor` has to ask first. Wayland has no global coordinates, and
/// GTK's Windows and macOS backends fall back to a monitor themselves, so `None` there
/// leaves anchoring exactly as it was.
pub(crate) fn surface_to_screen(surface: &gtk::gdk::Surface, x: i32, y: i32) -> Option<(i32, i32)> {
    platform_surface_to_screen(surface, x, y)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_surface_to_screen(surface: &gtk::gdk::Surface, x: i32, y: i32) -> Option<(i32, i32)> {
    x11::surface_to_screen(surface, x, y)
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
fn platform_surface_to_screen(surface: &gtk::gdk::Surface, x: i32, y: i32) -> Option<(i32, i32)> {
    let _ = (surface, x, y);
    None
}

// ── which process holds the front (tests only) ────────────────────────────────────

/// The pid of the frontmost application, for a GTK test that needs an active window and
/// must tell another application holding the front (a reason to skip) from its own window
/// failing to activate (a failure).
///
/// Only macOS answers, because only there was it seen (the suite running behind the
/// developer's other work). Elsewhere `None`: on the WM-less Xvfb gate a mapped window is
/// active at once (GTK4Rs/AP-119), so no other application can be in front, and a test
/// reading `None` treats an inactive window as the failure it is there.
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) fn frontmost_pid() -> Option<i32> {
    platform_frontmost_pid()
}

/// Whether `pid` launched this process, directly or through its own children: the
/// terminal a test suite was started from, which a window that activates takes the
/// front from.
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) fn is_ancestor_process(pid: i32) -> bool {
    platform_is_ancestor_process(pid)
}

#[cfg(all(test, feature = "gtk-integration-tests", target_os = "macos"))]
fn platform_frontmost_pid() -> Option<i32> {
    mac::frontmost::frontmost_pid()
}

#[cfg(all(test, feature = "gtk-integration-tests", target_os = "macos"))]
fn platform_is_ancestor_process(pid: i32) -> bool {
    mac::frontmost::is_ancestor(pid)
}

#[cfg(all(test, feature = "gtk-integration-tests", not(target_os = "macos")))]
fn platform_frontmost_pid() -> Option<i32> {
    None
}

#[cfg(all(test, feature = "gtk-integration-tests", not(target_os = "macos")))]
fn platform_is_ancestor_process(_pid: i32) -> bool {
    false
}

// ── the system "reduce animations" preference ───────────────────────────────────
//
// A second façade beside the per-platform modules above, living here rather than in
// either child, because it has THREE arms rather than two: Windows has a source
// (`win32::reduced_motion`), macOS has one too but wiring it is deliberately
// deferred (it shares the no-source arm below), and every other platform — Linux included,
// where `GtkSettings:gtk-enable-animations` already carries the desktop's answer —
// has none at all. `animation::policy` is the one caller and the one place that
// turns this into a decision (POLICY § Platform seams: a seam supplies a source and
// owns no behaviour).

/// A live subscription to [`system_reduced_motion`] changing. Dropping it
/// unsubscribes. Each platform supplies its own teardown — a GLib timeout source
/// removed on Windows, nothing to do where there is no source at all — behind one
/// droppable type so `animation::policy::watch` holds a single guard regardless of
/// platform, exactly as it already does for the reader's own choice and for
/// `gtk-enable-animations`.
pub(crate) struct ReducedMotionWatch(Option<Box<dyn FnOnce()>>);

impl ReducedMotionWatch {
    /// Build a guard whose `teardown` runs once, on drop.
    fn new(teardown: impl FnOnce() + 'static) -> Self {
        Self(Some(Box::new(teardown)))
    }

    /// A subscription over nothing: never fires, nothing to unregister. The source
    /// half for every platform with no source of its own (Linux, and macOS while its
    /// module is deferred). `#[cfg]`-gated to exactly those platforms so the Windows
    /// build, which never calls it, has no dead-code warning under `-D warnings`.
    #[cfg(not(windows))]
    fn noop() -> Self {
        Self(None)
    }
}

impl Drop for ReducedMotionWatch {
    fn drop(&mut self) {
        if let Some(teardown) = self.0.take() {
            teardown();
        }
    }
}

/// Whether the OS is currently asking for reduced motion, or `None` if this
/// platform has no source of its own to ask. `Some(true)` means the system wants
/// LESS animation; `Some(false)` that it explicitly does not; `None` must never be
/// treated as either — a caller that cannot tell must leave its other inputs
/// standing rather than guess (mirrors how `animation::policy::enabled_of` treats a
/// missing `GtkSettings` as GTK's own default rather than as a guessed answer).
///
/// - **Windows**: `SPI_GETCLIENTAREAANIMATION` — see
///   `platform::win32::reduced_motion` for the exact API and why not its neighbour
///   `SPI_GETANIMATION`.
/// - **macOS**: a real source exists —
///   `[[NSWorkspace sharedWorkspace] accessibilityDisplayShouldReduceMotion]`, with
///   change notification `NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification`
///   posted on `[NSWorkspace sharedWorkspace].notificationCenter` — but wiring it is
///   deliberately DEFERRED: no seat can compile or verify Objective-C runtime FFI
///   for it right now. Until it lands, macOS answers `None` here exactly like a
///   platform with no source at all, so `gtk-enable-animations` stays hardcoded
///   `TRUE` there and nothing about this platform's behaviour changes yet.
/// - **Linux and everything else**: `None`, always — `GtkSettings` already carries
///   the desktop's answer, so there is nothing for this façade to add.
pub(crate) fn system_reduced_motion() -> Option<bool> {
    platform_reduced_motion()
}

/// Subscribe to [`system_reduced_motion`] changing. `f` is called with the NEW
/// value whenever the platform's source changes; dropping the returned guard
/// unsubscribes. On a platform with no source (Linux, and macOS until its module
/// lands — see [`system_reduced_motion`]), the source half never fires: `f` is
/// still accepted rather than refused, so a caller never needs a platform match of
/// its own.
///
/// On EVERY platform a subscription has two halves: the platform's own source, and
/// an entry in [`listeners`]. The entry lets a test signal a change without a real
/// OS setting to flip, through the same façade on Windows, macOS and Linux. That way
/// a test of a caller's subscription compiles and runs on every platform instead of
/// only one (POLICY: never `#[cfg(platform)]` a test). Dropping the guard tears down
/// both halves.
pub(crate) fn watch_reduced_motion(f: impl Fn(Option<bool>) + 'static) -> ReducedMotionWatch {
    let f: listeners::Listener = std::rc::Rc::new(f);
    listeners::add(&f);
    let source = platform_watch_reduced_motion({
        let f = f.clone();
        move |value| f(value)
    });
    ReducedMotionWatch::new(move || {
        drop(source);
        listeners::remove(&f);
    })
}

#[cfg(windows)]
fn platform_reduced_motion() -> Option<bool> {
    win32::system_reduced_motion()
}

#[cfg(windows)]
fn platform_watch_reduced_motion(f: impl Fn(Option<bool>) + 'static) -> ReducedMotionWatch {
    win32::watch_reduced_motion(f)
}

// Every platform with no source of its own: Linux, anything else that is not
// Windows, and macOS. macOS does have a real source, but wiring it is deliberately
// deferred (see `system_reduced_motion`'s doc comment above for the exact API and
// notification name) because no seat can compile or verify Objective-C runtime FFI
// for it right now. When the macOS module lands, macOS gets its own arm, as Windows
// already has.
#[cfg(not(windows))]
fn platform_reduced_motion() -> Option<bool> {
    None
}

#[cfg(not(windows))]
fn platform_watch_reduced_motion(f: impl Fn(Option<bool>) + 'static) -> ReducedMotionWatch {
    let _ = f;
    ReducedMotionWatch::noop()
}

/// Every live [`watch_reduced_motion`] subscriber, on every platform. Production code
/// only adds and removes entries; the one thing that fires them is
/// [`testing::inject_reduced_motion`]. No host can produce a real Windows poll tick or
/// macOS notification from inside a test, so a test drives this list instead. That
/// proves a caller really subscribed through the façade and re-fires when a change
/// arrives. It does not test any platform's OS mechanism; each platform seat verifies
/// that separately (POLICY: verification is per-platform).
mod listeners {
    use std::cell::RefCell;
    use std::rc::Rc;

    /// One registered subscriber. Named so `LISTENERS` below reads as a plain
    /// collection rather than tripping `clippy::type_complexity`.
    pub(super) type Listener = Rc<dyn Fn(Option<bool>)>;

    thread_local! {
        static LISTENERS: RefCell<Vec<Listener>> = RefCell::new(Vec::new());
    }

    pub(super) fn add(f: &Listener) {
        LISTENERS.with(|l| l.borrow_mut().push(f.clone()));
    }

    pub(super) fn remove(f: &Listener) {
        LISTENERS.with(|l| l.borrow_mut().retain(|other| !Rc::ptr_eq(other, f)));
    }

    /// TEST-ONLY: call every live subscriber with `value`, as a real Windows poll
    /// tick (or a future macOS notification callback) would when the OS setting
    /// changes. It deliberately does NOT change what [`super::system_reduced_motion`]
    /// returns. A test that uses this is testing the subscription plumbing, not a
    /// made-up value that the production build could never return.
    ///
    /// `pub(crate)`, not `pub(super)` like its neighbours: it is re-exported through
    /// [`super::testing`] for `animation::policy`'s tests, which live outside
    /// `platform` entirely. A re-export can never widen an item's own visibility.
    /// It uses the same cfg as its callers rather than a bare `#[cfg(test)]` (POLICY §
    /// GTK-object integration tests).
    #[cfg(all(test, feature = "gtk-integration-tests"))]
    pub(crate) fn inject(value: Option<bool>) {
        let snapshot: Vec<_> = LISTENERS.with(|l| l.borrow().clone());
        for f in snapshot {
            f(value);
        }
    }
}

/// Test-only seam onto [`listeners::inject`], so `animation::policy`'s own tests can
/// signal a reduced-motion change without reaching into a private sibling module.
/// See [`listeners`]'s doc comment for why simulating the SUBSCRIPTION firing is the
/// honest test. Present on every platform, like its callers.
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) mod testing {
    pub(crate) use super::listeners::inject as inject_reduced_motion;
}

// ── this process's memory footprint (the memory gates' instrument) ──────────────────

/// This process's memory footprint in bytes, by each platform's own measure (Linux
/// VmRSS, macOS physical footprint, Windows working set), or `None` where there is no
/// reader. Test-only: the memory gates are its one caller.
#[cfg(test)]
pub(crate) fn process_footprint_bytes() -> Option<u64> {
    platform_process_footprint_bytes()
}

#[cfg(all(test, target_os = "linux"))]
fn platform_process_footprint_bytes() -> Option<u64> {
    linux::process::resident_bytes()
}

#[cfg(all(test, target_os = "macos"))]
fn platform_process_footprint_bytes() -> Option<u64> {
    mac::process::phys_footprint_bytes()
}

#[cfg(all(test, windows))]
fn platform_process_footprint_bytes() -> Option<u64> {
    win32::process::current_working_set_bytes()
}

#[cfg(all(test, not(any(target_os = "linux", target_os = "macos", windows))))]
fn platform_process_footprint_bytes() -> Option<u64> {
    None
}

/// Opt this process in or out of the kernel collapsing its memory into huge pages,
/// which moves a footprint reading with no allocation at all. Only Linux has the
/// collapse; elsewhere this does nothing.
#[cfg(all(test, feature = "memory-gates"))]
pub(crate) fn set_huge_page_collapse_disabled(disabled: bool) {
    platform_set_huge_page_collapse_disabled(disabled);
}

#[cfg(all(test, feature = "memory-gates", target_os = "linux"))]
fn platform_set_huge_page_collapse_disabled(disabled: bool) {
    linux::process::set_huge_page_collapse_disabled(disabled);
}

#[cfg(all(test, feature = "memory-gates", not(target_os = "linux")))]
fn platform_set_huge_page_collapse_disabled(disabled: bool) {
    let _ = disabled;
}

// ── the SVG decoder this process loads ───────────────────────────────────────────────

/// Whether the SVG decoder this process loads is one this project delivered with the
/// XInclude flaw fixed (CVE-2026-96889), so the image screen may stand down
/// (`imagedecode::xinclude::enforced`). Read once per process: what is loaded cannot
/// change while it runs.
///
/// Only a macOS bundle answers `true`: it stages its own librsvg and its build gate
/// holds that copy to the floor. Everywhere else, a macOS binary run outside its bundle
/// included, the decoder is the system's or a package manager's, and this is `false`.
pub(crate) fn svg_decoder_is_delivered_fixed() -> bool {
    static FIXED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FIXED.get_or_init(platform_svg_decoder_is_delivered_fixed)
}

#[cfg(target_os = "macos")]
fn platform_svg_decoder_is_delivered_fixed() -> bool {
    mac::bundle::carries_verified_librsvg()
}

#[cfg(not(target_os = "macos"))]
fn platform_svg_decoder_is_delivered_fixed() -> bool {
    false
}

// ── directories that mount something when a name inside them is looked up ──────────

/// Whether `dir` is an automounter's directory: one where looking up any name inside it
/// makes the system mount whatever that name denotes, which for a map like macOS's
/// `/net -hosts` means contacting the host the name spells. Answered about `dir` itself,
/// without looking anything up beneath it.
///
/// `dir` must already exist and contain no unexamined symbolic link (the image gate's
/// link walk only asks about the prefix it has already resolved).
///
/// **The one question, answered two ways.** Both platforms answer "can a lookup inside
/// `dir` mount something?", each from the source that is right for its automounter:
///
/// | | Linux | macOS |
/// |---|---|---|
/// | read from | the mount table: an `autofs` mount AT `dir` | `statfs(dir)`: `dir` is ON an `autofs` filesystem |
/// | a map's own directory (`/net`) | yes | yes |
/// | a directory inside a map, not yet mounted | no — the walk is refused at the map's own directory before it gets there | yes, if it is still on the map's `autofs` (not measured) |
/// | a direct map's point, already mounted | yes — `statfs` would report the filesystem on top | no, as `statfs` reports what is on top (not measured on macOS): the walk goes on into a mount that is already there, so the host it contacts is one already contacted |
/// | the source cannot be read | the stock maps (`/net`, `/misc`) are assumed, so a hidden `/proc` cannot open `/net` | `dir` cannot be reached, so nor can a name inside it: the lookup this guards fails without contacting anything |
///
/// The last row is what keeps either answer from failing open. The two differ only where
/// a lookup cannot reach a host not already reached, and macOS keeps `statfs` because it
/// answers about the path the walk actually holds: the walk reaches the hosts map as
/// `/net`, and `statfs` answered `autofs` for both `/net` and `/System/Volumes/Data/net`
/// (MEASURED, `mac/automount.rs`), while a mount-table match on `f_mntonname` would have
/// to equal whichever of the two spellings the table records.
///
/// Elsewhere this is `false`, which is what the gate did before.
pub(crate) fn mounts_on_lookup(dir: &std::path::Path) -> bool {
    platform_mounts_on_lookup(dir)
}

#[cfg(target_os = "macos")]
fn platform_mounts_on_lookup(dir: &std::path::Path) -> bool {
    mac::automount::is_automount_directory(dir)
}

#[cfg(target_os = "linux")]
fn platform_mounts_on_lookup(dir: &std::path::Path) -> bool {
    linux::automount::is_automount_directory(dir)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn platform_mounts_on_lookup(dir: &std::path::Path) -> bool {
    let _ = dir;
    false
}

#[cfg(test)]
mod mounts_on_lookup_tests {
    /// The rows of `mounts_on_lookup`'s contract that every platform answers alike, run
    /// against each platform's own source: an ordinary directory and a missing one are
    /// not directories whose lookups mount anything. The rows where the platforms
    /// differ are each module's own tests.
    #[test]
    fn ordinary_and_unreachable_directories_mount_nothing_on_any_platform() {
        let dir = tempfile::tempdir().unwrap();
        for path in [dir.path().to_path_buf(), dir.path().join("absent")] {
            assert!(!super::mounts_on_lookup(&path), "{path:?}");
        }
    }
}
