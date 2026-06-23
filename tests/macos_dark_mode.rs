//! macOS dark mode: the two links the unit tests cannot reach.
//!
//! `src/platform/mac/appearance.rs` unit-tests its polarity, but only the parts
//! that touch no GTK — because on macOS nothing that touches GTK can be a unit
//! test at all: libtest runs every body on a worker thread and GTK refuses to
//! initialise off the process main thread. That leaves the load-bearing claim
//! untested by construction: **that writing `gtk-application-prefer-dark-theme`
//! actually changes anything on the Quartz backend.** Detection that writes into
//! a void would pass every unit test in the module and leave the app permanently
//! light, which is the exact bug being fixed.
//!
//! So this is a `harness = false` target, per `Cargo.toml` and `tests/icon_resolution.rs`:
//! cargo runs this file's own `main()` on the process main thread, so GTK
//! initialises normally.
//!
//! It **stays** one, now that `src/gtk_suite.rs` runs every `#[gtktest::test]` body on
//! the main thread too, because of what this file does to reach its conclusion: it
//! drives `gtk-application-prefer-dark-theme` in *both* directions, deliberately, so
//! that a property being ignored outright cannot pass by leaving the machine's own
//! setting in place. Those writes are process-global and GTK cannot be
//! un-initialised, so in the shared suite they would reach every body that ran after
//! it. The isolation is the point; do not fold this in.
//!
//! Run: `cargo test --features gtk-integration-tests --test macos_dark_mode`

// The module under test, pulled in by path. `#[path]` rather than `include!` because
// the file opens with `//!` inner doc comments, which a macro expansion may not
// introduce.
//
// The crate IS a library now, but its tree is `pub(crate)` and `pub use` cannot widen
// that (E0364), so an external target still cannot import it. This target keeps its
// own process on purpose — see the header — rather than joining the main-thread suite
// in `src/gtk_suite.rs`, which could reach the module directly.
#[cfg(target_os = "macos")]
#[path = "../src/platform/mac/appearance.rs"]
mod appearance;

// The shared writer `appearance` hands the lightness to — which settings carry it,
// and in what order. Pulled in beside the module under test rather than stubbed,
// because it is the half this file's § 4 exists to assert: it is what decides
// whether the window PAINTS what the appearance says. Declared at this crate root so
// `appearance`'s own `crate::colorscheme::…` resolves here exactly as it does in the
// real crate. It is a deliberate leaf (see its header) so this costs nothing else.
#[cfg(target_os = "macos")]
#[path = "../src/colorscheme.rs"]
mod colorscheme;

/// On any other platform this target is vacuous but must still link — a
/// `harness = false` target with no `main` fails to build.
#[cfg(not(target_os = "macos"))]
fn main() {
    println!("macos_dark_mode: macOS-only check, nothing to do on this platform.");
}

#[cfg(target_os = "macos")]
fn main() {
    gtk::init().expect("GTK init on the process main thread");
    let settings = gtk::Settings::default().expect("a default GtkSettings");

    // A widget to read resolved theme colours from, exactly as
    // `palette::desktop_is_dark` does.
    let probe = gtk::Box::new(gtk::Orientation::Horizontal, 0);

    let mut failures: Vec<String> = Vec::new();

    // ── 1. The platform assumption ──────────────────────────────────────────
    //
    // Both directions, deliberately. Asserting only the state the machine
    // happens to be in would pass on a light desktop even if the property were
    // ignored outright — GTK's default is already light, so "still light" proves
    // nothing. Driving it makes the check independent of the operator's setting.
    //
    // ⚠ Scope, stated precisely because it was overstated once already: this
    // asserts that the RESOLVED PALETTE follows the property — what
    // `palette::desktop_is_dark()` reads and what the generated theme sheet is
    // built from. It does **not** assert that the window is painted dark. On this
    // build those are measurably different things: `theme_base_color` follows
    // while the rendered window does not. Do not read a pass here as "dark mode
    // renders".
    for want_dark in [true, false] {
        settings.set_gtk_application_prefer_dark_theme(want_dark);
        let dark = probe_is_dark(&probe);
        println!(
            "  prefer-dark={:<5} -> theme reads {}",
            want_dark,
            if dark { "DARK" } else { "LIGHT" }
        );
        if dark != want_dark {
            failures.push(format!(
                "setting gtk-application-prefer-dark-theme={want_dark} did not move the resolved \
                 palette: theme_base_color still reads {}. palette::desktop_is_dark() can then \
                 never become {want_dark}, so every colour derived from it is stuck regardless of \
                 how correctly the OS setting is read.",
                if dark { "dark" } else { "light" }
            ));
        }
    }

    // ── 2. The module end to end ────────────────────────────────────────────
    //
    // Whatever the machine is set to right now, the entry point the app calls at
    // startup must leave GTK agreeing with the OS. This is the whole feature in
    // one assertion, and it needs no control over the operator's setting.
    let os_dark = appearance::macos_prefers_dark();
    // Start from the wrong value, so a `track_system_dark_mode` that did nothing
    // at all cannot pass by inheriting a value that happened to be right.
    settings.set_gtk_application_prefer_dark_theme(!os_dark);
    appearance::track_system_dark_mode();
    let gtk_dark = settings.is_gtk_application_prefer_dark_theme();
    println!(
        "\n  macOS says {}, GTK now says {}",
        label(os_dark),
        label(gtk_dark)
    );
    if gtk_dark != os_dark {
        failures.push(format!(
            "after track_system_dark_mode(), GTK reads {} but macOS is {}",
            label(gtk_dark),
            label(os_dark)
        ));
    }
    if probe_is_dark(&probe) != os_dark {
        failures.push(format!(
            "the resolved theme reads {} but macOS is {} — the property moved, the theme did not",
            label(probe_is_dark(&probe)),
            label(os_dark)
        ));
    }

    // ── 3. LIVE delivery ────────────────────────────────────────────────────
    //
    // The regression this exists for: the first version of the module deferred
    // its work to `glib::idle_add_once`, which on an idle Quartz app is not
    // dispatched for many seconds — so a real flip was answered with a reading of
    // the appearance taken long afterwards, i.e. often the opposite one. Startup
    // detection still passed every other check here, which is precisely why this
    // one is separate: it is the only assertion that exercises the observer.
    //
    // The notification is posted rather than the machine's real appearance being
    // flipped: a test must not change the operator's system settings. Other
    // observers on the machine simply re-read the (unchanged) real setting and
    // no-op.
    settings.set_gtk_application_prefer_dark_theme(!os_dark);
    let main_loop = glib::MainLoop::new(None, false);
    glib::timeout_add_local_once(
        std::time::Duration::from_millis(200),
        post_appearance_notification,
    );
    glib::timeout_add_local_once(std::time::Duration::from_millis(1500), {
        let main_loop = main_loop.clone();
        move || main_loop.quit()
    });
    main_loop.run();

    let after = settings.is_gtk_application_prefer_dark_theme();
    println!(
        "  posted a synthetic appearance notification: GTK went {} -> {}",
        label(!os_dark),
        label(after)
    );
    if after != os_dark {
        failures.push(format!(
            "the appearance observer did not take effect: GTK still reads {} after the \
             notification, expected {}. Either the observer is not registered, or its work is \
             being deferred to a main-loop turn that does not come.",
            label(after),
            label(os_dark)
        ));
    }

    // ── 4. THE PIXELS ───────────────────────────────────────────────────────
    //
    // The assertion ScrAP-184 says every one of the checks above was missing: it
    // renders a real window through GSK and samples what comes out. Checks 1–3
    // all read intermediate state, and intermediate state stayed correct through
    // an entire session in which the app displayed the wrong colours.
    //
    // Rendered through `GskCairoRenderer` into a texture and sampled directly,
    // rather than screenshotted. Screen capture proved unreliable here — it
    // repeatedly grabbed a desktop the window was not on, which reads exactly
    // like "the app is light" and cost real time. A texture sample cannot land on
    // the wrong Space.
    for want_dark in [true, false] {
        // Through the shared writer, so this samples the code the app runs rather
        // than a test-local reconstruction of it. Forced the wrong way first: the
        // writer is change-gated, and starting from the value being asked for would
        // let a writer that did nothing pass.
        settings.set_gtk_application_prefer_dark_theme(!want_dark);
        colorscheme::apply_desktop_lightness(want_dark);
        match render_centre_pixel() {
            Some((r, g, b)) => {
                let painted_dark = (u32::from(r) + u32::from(g) + u32::from(b)) / 3 < 110;
                println!(
                    "  dark={want_dark:<5} -> window paints rgb({r},{g},{b}) = {}",
                    if painted_dark { "DARK" } else { "LIGHT" }
                );
                if painted_dark != want_dark {
                    failures.push(format!(
                        "with dark={want_dark} the window PAINTS rgb({r},{g},{b}). The settings \
                         and the resolved palette can be perfectly correct and this still fail — \
                         that is the whole reason this check exists."
                    ));
                }
            }
            None => failures.push(
                "the window produced no render node at all — cannot judge the painted result"
                    .into(),
            ),
        }
    }

    // ── informational ───────────────────────────────────────────────────────
    //
    // Not an assertion. If a future GTK started reading the macOS appearance
    // itself this module would become redundant, and that is worth noticing —
    // but it is not a regression, so it must not fail the build.
    println!(
        "\n  (GTK 4.22.4 does not read the macOS appearance itself; this module supplies it. \
         Installed: {}.{}.{})",
        gtk::major_version(),
        gtk::minor_version(),
        gtk::micro_version()
    );

    if !failures.is_empty() {
        eprintln!("\nFAILED:");
        for f in &failures {
            eprintln!("  - {f}");
        }
        std::process::exit(1);
    }
    println!("\nmacOS dark-mode tracking holds in both directions.");
}

/// Build a small offscreen window, render it through GSK, and return the centre
/// pixel — the app's own painted output, with no screen capture in the loop.
///
/// Uses the Cairo renderer specifically: that is the one `main.rs` pins for the
/// shipped binary, so this samples the renderer the product actually uses.
#[cfg(target_os = "macos")]
fn render_centre_pixel() -> Option<(u8, u8, u8)> {
    use gtk::prelude::*;

    let window = gtk::Window::new();
    window.set_default_size(300, 200);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    content.set_hexpand(true);
    content.set_vexpand(true);
    content.append(&gtk::Label::new(Some("paint check")));
    window.set_child(Some(&content));
    // Presented, briefly. A realized-but-unmapped window has no allocation and
    // snapshots to nothing at all — measured, and it fails as "no render node"
    // rather than as a wrong colour, which is at least honest. The window is
    // destroyed a few hundred milliseconds later.
    window.present();
    let pump = glib::MainLoop::new(None, false);
    glib::timeout_add_local_once(std::time::Duration::from_millis(400), {
        let pump = pump.clone();
        move || pump.quit()
    });
    pump.run();

    let paintable = gtk::WidgetPaintable::new(Some(&window));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(snapshot.upcast_ref::<gtk::gdk::Snapshot>(), 300.0, 200.0);
    let node = snapshot.to_node()?;

    let renderer = gtk::gsk::CairoRenderer::new();
    renderer.realize(None::<&gtk::gdk::Surface>).ok()?;
    let texture = renderer.render_texture(&node, None);

    let (w, h) = (texture.width() as usize, texture.height() as usize);
    let mut data = vec![0u8; w * h * 4];
    texture.download(&mut data, w * 4);
    let idx = ((h / 2) * w + w / 2) * 4;
    // Cairo ARGB32 on a little-endian host: B, G, R, A.
    let px = (data[idx + 2], data[idx + 1], data[idx]);
    window.destroy();
    Some(px)
}

/// Post the same distributed notification macOS posts on an appearance flip.
///
/// Declared here rather than in the module under test because the app only ever
/// *observes* this notification — a `post` entry point in production code would
/// exist for no reason but this test, which is how test scaffolding ends up
/// shipping.
#[cfg(target_os = "macos")]
fn post_appearance_notification() {
    use std::ffi::c_void;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringCreateWithBytes(
            alloc: *const c_void,
            bytes: *const u8,
            num_bytes: isize,
            encoding: u32,
            is_external_representation: u8,
        ) -> *const c_void;
        fn CFRelease(cf: *const c_void);
        fn CFNotificationCenterGetDistributedCenter() -> *const c_void;
        fn CFNotificationCenterPostNotification(
            center: *const c_void,
            name: *const c_void,
            object: *const c_void,
            user_info: *const c_void,
            deliver_immediately: u8,
        );
    }

    const NAME: &str = "AppleInterfaceThemeChangedNotification";
    // SAFETY: the byte range is a live Rust string of the stated length, and the
    // created string is released below after the post returns.
    unsafe {
        let name = CFStringCreateWithBytes(
            std::ptr::null(),
            NAME.as_ptr(),
            NAME.len() as isize,
            0x0800_0100, // kCFStringEncodingUTF8
            0,
        );
        if name.is_null() {
            return;
        }
        CFNotificationCenterPostNotification(
            CFNotificationCenterGetDistributedCenter(),
            name,
            std::ptr::null(),
            std::ptr::null(),
            1,
        );
        CFRelease(name);
    }
}

#[cfg(target_os = "macos")]
fn label(dark: bool) -> &'static str {
    if dark {
        "dark"
    } else {
        "light"
    }
}

/// The same probe `palette::desktop_is_dark` performs.
///
/// Deliberately a copy, and only of the *platform* reading — not of app logic.
/// `palette.rs` cannot be pulled in by path here without dragging `config` and
/// the whole `renderer`/syntect tree in behind it, and what this target needs to
/// assert is GTK's behaviour, not the app's arithmetic (which the Linux suite
/// already covers headlessly).
#[cfg(target_os = "macos")]
#[allow(deprecated)] // style_context(): deprecated in GTK >= 4.10, no stable alternative
fn probe_is_dark(probe: &gtk::Box) -> bool {
    use gtk::prelude::*;

    let ctx = probe.style_context();
    let bg = ctx
        .lookup_color("theme_base_color")
        .or_else(|| ctx.lookup_color("view_bg_color"))
        .or_else(|| ctx.lookup_color("theme_bg_color"))
        .unwrap_or_else(|| gtk::gdk::RGBA::new(1.0, 1.0, 1.0, 1.0));

    let lin = |x: f32| -> f64 {
        let x = x as f64;
        if x <= 0.03928 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = 0.2126 * lin(bg.red()) + 0.7152 * lin(bg.green()) + 0.0722 * lin(bg.blue());
    luminance < 0.5
}
