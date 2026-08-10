//! Scribobulate — native GTK4 Markdown viewer/editor.
//!
//! Renders Markdown into a single GtkTextView with GtkTextTags for formatting,
//! giving continuous cross-document text selection. No HTML engine, no GPU memory.
//!
//! # Why this is a library crate with a three-line `main.rs`
//!
//! This file — not `main.rs` — is the crate root, and it owns the module list.
//! `src/main.rs` shrank to argument-free delegation into [`run`].
//!
//! The reason is testing, not layering. A **binary** crate exposes no importable
//! surface: nothing can link it, so a `tests/*.rs` target cannot reach one line of
//! it. That matters because gtk4-rs dispatches `#[gtk::test]` bodies onto a glib
//! `ThreadPool` worker, and a GTK build that requires initialisation on the process
//! main thread (macOS/Quartz) cannot run them at all — so a GTK assertion that must
//! hold on every platform has to live in a target that owns its own `main()`. While
//! the crate was bin-only, such a target could only re-include a source file by
//! `#[path]`, compiling it a second time as an unrelated module, which works for a
//! leaf file and not for anything with structure.
//!
//! **The seam that solves it is `src/gtk_suite.rs`, not this file's public API.** An
//! ordinary `tests/*.rs` integration test links this crate *externally*, so it sees
//! only `pub` items — and the module tree below is deliberately `pub(crate)`
//! throughout (a `pub use` re-export cannot widen that: rustc rejects it with
//! E0364/E0365). Widening ~900 items to `pub` on a crate that publishes nothing
//! would be a worse trade than the `#[path]` hack it replaced. So the test *runner*
//! is instead a **second crate root**, `src/gtk_suite.rs` — compiled `--cfg test`
//! from the same sources, beside this file, so it sees the whole `pub(crate)` tree
//! directly with no widening and no `pub` seam of any kind. Its one real cost is
//! re-declaring this file's module list (`scripts/lint-references.sh` checks the two
//! stay in sync) and compiling the tree a second time — paid deliberately, so that
//! nothing built for testing ever compiles into the shipped library.

pub(crate) mod a11y;
pub(crate) mod annotate;
pub(crate) mod annotations;
pub(crate) mod annotations_view;
pub(crate) mod app;
pub(crate) mod atomic_io;
pub(crate) mod codeview;
// The write side of the desktop light/dark channel, shared by the two platform
// modules that have to supply that channel's missing SOURCE (both under `platform/`).
// Gated at the module for the same reason those are: on Linux the desktop writes the
// setting itself, so nothing here has a caller and the `-D warnings` clippy gate would
// have to be bought off with an `#[allow]`.
#[cfg(any(windows, target_os = "macos"))]
pub(crate) mod colorscheme;
pub(crate) mod config;
pub(crate) mod copymap;
pub(crate) mod docio;
pub(crate) mod farscroll;
pub(crate) mod forensics;
pub(crate) mod format;
pub(crate) mod icons;
pub(crate) mod keynav;
pub(crate) mod limits;
pub(crate) mod links;
pub(crate) mod logging;
pub(crate) mod outline;
pub(crate) mod outline_view;
pub(crate) mod palette;
/// Per-platform seams, one directory per target OS. Each child module is
/// `#[cfg]`-gated inside `platform/mod.rs`, at its own declaration.
pub(crate) mod platform;
pub(crate) mod preview;
pub(crate) mod renderer;
pub(crate) mod saferizer;
pub(crate) mod session;
pub(crate) mod span;
/// The registry `#[gtktest::test]` submits into. Gated on `test` as well as the
/// feature so it never reaches the shipped library: a `harness = false` target is
/// built `--cfg test`, so this one gate covers both the lib-test target and the
/// suite crate (`src/gtk_suite.rs`).
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) mod suite_registry;
/// Crash-recovery swap files: the display-free header codec, naming, digest and
/// recovery decisions. The GTK/filesystem edges are in `window/swap*.rs`.
pub(crate) mod swapfile;
pub(crate) mod tags;
pub(crate) mod tasklist;
/// Test-only. Shared symlink setup with a runtime skip, so a test whose subject is a
/// symlink is *skipped and counted* where the platform refuses one rather than
/// `#[cfg(unix)]`-deleted (ScrAP-212). Not gated on the GTK-suite feature: its
/// consumers are ordinary unit tests.
#[cfg(test)]
pub(crate) mod testsymlink;
pub(crate) mod theme;
pub(crate) mod widgets;
pub(crate) mod window;
pub(crate) mod winstate;
// Linux-only by construction: every path in it is XDG/X11 desktop plumbing —
// `~/.XCompose`, `$XDG_CONFIG_HOME`, `mimeapps.list`, `/etc/keyd` — wired together
// with `std::os::unix` symlinks that do not exist on other targets. Gating the
// module rather than its internals keeps the non-unix build free of dead code, so
// the `-D warnings` clippy gate stays satisfiable without an `#[allow]`.
#[cfg(unix)]
pub(crate) mod workaround;

use gtk::gio::ApplicationFlags;
use gtk::prelude::*;
use gtk::{glib, Application};

/// The application ID. Defined in `icons.rs` beside the literal it derives from,
/// and re-exported — not re-declared — so this root and `src/gtk_suite.rs` cannot
/// drift. See [`icons::APP_ID`].
pub(crate) use icons::APP_ID;

/// The whole program, less the process entry point.
///
/// `main.rs` does nothing but call this and return its exit code. Everything here
/// ran verbatim in `main()` before the library split, and the ordering constraints
/// documented inline are the reason it is one function rather than several: several
/// of these steps must happen before GTK or GLib initialises, and a caller that
/// reordered them would re-arm bugs that took a long time to find.
pub fn run() -> glib::ExitCode {
    // Force the GSK Cairo software renderer: no GL/GLES context, no GPU memory.
    // Must be set before GTK initialises (POLICY.md architecture rule).
    //
    // Ahead of `logging::init()` rather than after it, which is a weaker constraint
    // than it looks: the requirement is only "before GTK initialises", and logging
    // touches glib's log writer, never GTK or GSK. Moving it up buys a real thing —
    // the crash-forensics identity stamp *records* the active renderer, and a stamp
    // written before this line reported `(unset)` on every run, which is precisely
    // the sort of quietly-wrong field that gets believed in a post-mortem.
    std::env::set_var("GSK_RENDERER", "cairo");

    // Initialise logging next, so startup diagnostics (and any glib/GTK messages)
    // are captured. Single `RUST_LOG`-controlled sink; see src/logging.rs. This also
    // installs the crash-forensics kit (persistent log, breadcrumb ring, panic and
    // fatal-signal handlers), so everything below is covered by it.
    logging::init();

    // Snapshot the user config dir before anything touches XDG_CONFIG_HOME.
    //
    // The real constraint is stronger than "config must be read first": **no GLib
    // read of the config dir may happen before the redirect below, or the XCompose
    // workaround breaks.** `g_get_user_config_dir()` caches its answer in a global
    // static forever on FIRST call, and GTK 4.6's compose table reads that same
    // global — so an early GLib read would cache the REAL dir and re-arm the crash
    // the redirect exists to prevent, while a late one resolves into the temp dir
    // and silently loses the user's config and theme overrides. There is no ordering
    // that gives both, which is why `config::user_config_dir` hand-rolls the lookup
    // from `std::env` instead. See its doc comment.
    let _ = config::config();

    // Windows: take the real Win32 window frame instead of GTK's client-side
    // decorations. GDK-Win32 defaults to CSD, so without this the app draws its own
    // GNOME-style titlebar and buttons — and gets no native resize borders, no
    // Alt+Space system menu, and no Snap Layouts. `GTK_CSD=0` restores all of those
    // (all measured; Snap Layouts confirmed by hand), and only works because this app
    // has no custom titlebar — no `GtkHeaderBar`, no `set_titlebar`. Adding one would
    // silently put CSD back. Set here rather than in a launcher so the frame is the
    // same however the binary is started, exactly as `GSK_RENDERER` is. Must precede
    // GTK init. (POLICY.md architecture rule, which also records why the alternatives
    // — an installer-set variable, restyled CSD, a forked chrome tree — were rejected).
    //
    // `GTK_CSD` is a GTK-wide variable, NOT a Windows one — this MUST stay
    // `cfg(windows)`. On Linux it would reach a live code path: X11 already uses
    // server-side decorations, but Wayland negotiates them with the compositor, and
    // forcing them off there can leave a window with no decorations at all. For the
    // same reason it must NOT go in `.cargo/config.toml`'s `[env]` table the way
    // `GSK_RENDERER` does — that table is platform-unconditional and would leak this
    // to every Linux `cargo test`/`cargo run`.
    #[cfg(windows)]
    std::env::set_var("GTK_CSD", "0");
    // No-op on non-unix targets: the GTK 4.6 compose-table crash this guards against
    // is reached through the X11 input-method path, and the workaround's mechanism
    // (redirecting `XDG_CONFIG_HOME` at a tree of symlinks) has no Windows analogue.
    // Nothing is lost by omitting it — the function already returns immediately on
    // GTK >= 4.12, and the Windows build is 4.22 (GTK4Rs/AP-3).
    #[cfg(unix)]
    workaround::workaround_gtk46_compose_crash();

    // `--new-instance` / `-n`: force a separate process instead of reusing the
    // running one (e.g. dev build alongside the installed app).  This MUST be
    // decided before the Application registers: g_application_run() forwards argv
    // to the existing primary instance *at registration*, which happens before any
    // activate/open/command-line handler runs — so a flag parsed there would simply
    // be forwarded and never spawn a new process (GTK4Rs/AP-17).  We therefore scan argv
    // ourselves, set NON_UNIQUE (same app-id, so icon/WM-class/settings identity is
    // unchanged — it just never does single-instance negotiation), and strip the
    // switch so the remaining args still flow into HANDLES_OPEN as file paths.
    let mut args: Vec<String> = std::env::args().collect();
    let force_new = args.iter().any(|a| a == "--new-instance" || a == "-n");
    args.retain(|a| a != "--new-instance" && a != "-n");

    let mut flags = ApplicationFlags::HANDLES_OPEN;
    if force_new {
        flags |= ApplicationFlags::NON_UNIQUE;
    }

    let app = Application::builder()
        .application_id(APP_ID)
        .flags(flags)
        .build();

    app::setup_app(&app);

    // On macOS the application-ID registration `run_with_args` performs below is
    // a no-op for UNIQUENESS: GIO negotiates that over a D-Bus session bus the
    // platform does not run, so every launch elects itself primary and TDD
    // §8.1/8.2 fail silently with two processes on one document — with no error
    // to catch, since "no peer found" and "no way to look" return the same value
    // (GTK4Rs/AP-157). `platform/mac/single_instance.rs` substitutes an equivalent
    // handoff and feeds it
    // into the very same `open`/`activate` handlers, so the behaviour below this
    // line is identical on both platforms.
    //
    // `--new-instance` opts out for exactly the reason it sets NON_UNIQUE: it
    // exists to run a dev build alongside the everyday one (TDD 8.5).
    //
    // The guard must outlive `run_with_args` — dropping it releases the lock and
    // unlinks the socket — so it is bound to a local that lives to the end of `run`.
    #[cfg(target_os = "macos")]
    let _single_instance = if force_new {
        None
    } else {
        use crate::platform::mac::single_instance::{elect, Launch};
        match elect(&app, args.get(1..).unwrap_or_default()) {
            Launch::Forwarded => return glib::ExitCode::SUCCESS,
            Launch::Primary(guard) => Some(guard),
        }
    };

    app.run_with_args(&args)
}
