//! Project-wide leveled logging.
//!
//! Single sink: the Rust `log` facade, rendered by `env_logger` and controlled at
//! runtime by `RUST_LOG`. glib/GTK's own diagnostics are bridged *into* that same
//! sink via a glib **writer** function, so application logs and GTK/glib messages
//! all flow through one configurable place. See POLICY.md (§Logging) and
//! ANTI-PATTERNS ScrAP-18.
//!
//! Call-site convention — use the plain `log` macros and let `target` default to
//! the module path (free per-module filtering):
//!
//! ```ignore
//! log::error!("save failed: {e}");          // non-fatal; app continues
//! log::warn!("missing config, using default");
//! log::info!("opened {path}");
//! log::debug!("entering split mode");
//! log::trace!(target: "scribobulate::scroll", "project_scroll: …"); // hot path
//! ```
//!
//! - `error!` is **non-fatal** ("failed but continue"). For genuinely fatal cases
//!   use `panic!`/`expect` (programmer error — unwinds, runs `Drop`) or
//!   `std::process::exit` (controlled shutdown after logging). Do **not** use glib's
//!   `g_error!` in app code — it `abort()`s with no unwinding.
//! - Reserve `trace!` for per-frame / per-event hot paths; disabled levels do not
//!   format their arguments, so they cost a branch and nothing else.
//! - Do not prefix permanent logs with `[🐛DEBUG]` — the level and module target
//!   already convey origin and severity. That prefix is only for throwaway
//!   `eprintln!` during active debugging.
//!
//! Runtime control (one knob for the whole process, app + GTK):
//!
//! ```text
//! RUST_LOG=warn                              # default
//! RUST_LOG=info,scribobulate=debug           # app at debug, GTK at info
//! RUST_LOG=warn,scribobulate::scroll=trace   # just the scroll hot path
//! ```

use gtk::glib;

use crate::forensics;

/// Known-benign GTK/GDK diagnostics that fire transiently during the FIRST layout /
/// surface-realization pass, originate inside GTK (not app code), and are
/// self-corrected — so they are noise at their native WARN/ERROR level. Matched by
/// substring and demoted to Debug in [`forward`]:
///
/// - `"sizes must be >= 0"` — the tail of GTK's single-line
///   `GtkGizmo 0x… (slider) reported min width -2, but sizes must be >= 0`: a
///   `GtkScrollbar` slider gizmo measuring a transient NEGATIVE min during the initial
///   measure pass (`SplitView::measure` → the always-on, `overlay_scrolling(false)`
///   preview scroller). One message, not two — matching this tail also suppresses the
///   `reported min width/height -2` head, which is why no separate substring is needed.
///   GTK clamps it to 0; there is no visual effect (verified). Not fixable app-side
///   (theme/GTK internal). Observed 4× at startup (width and height, both scrollers).
/// - `"GDK_IS_MONITOR"` — `gdk_monitor_get_geometry` called with a NULL monitor
///   while a surface is realized before it is assigned to a monitor (real
///   compositors only; never under Xvfb). GDK falls back; benign.
/// - `"Unable to connect to the accessibility bus"` — GTK unconditionally tries to
///   connect to the AT-SPI accessibility bus at startup; when no accessibility bus is
///   running (no screen reader / `at-spi-bus-launcher` active — the normal state on a
///   plain session) the connection fails and GTK logs this at ERROR. It is pure noise:
///   the app runs fully, only screen-reader integration is unavailable, and nothing is
///   using it. It CANNOT mask a real a11y failure for an AT user — a running screen
///   reader has the bus up, so those sessions never emit this message; only non-a11y
///   sessions see it. Demoting keeps a scary-looking ERROR off a default startup log.
///
/// Deliberately NARROW substrings so only these known transients are demoted —
/// every other GTK warning/critical still reaches WARN/ERROR.
fn is_benign_gtk_startup_noise(message: &str) -> bool {
    message.contains("sizes must be >= 0")
        || message.contains("GDK_IS_MONITOR")
        || message.contains("Unable to connect to the accessibility bus")
}

/// Forward one glib structured-log record into the Rust `log` crate.
///
/// glib routes all logging — legacy *and* structured (e.g. `Gtk-CRITICAL`) — through
/// the single writer func, which is why we bridge here rather than via
/// `log_set_default_handler` (that misses structured records — gtk4-rs #1957).
fn forward(level: glib::LogLevel, fields: &[glib::LogField<'_>]) -> glib::LogWriterOutput {
    let mut domain = "glib";
    let mut message = String::new();
    for f in fields {
        match f.key() {
            "MESSAGE" => message = f.value_str().unwrap_or_default().to_owned(),
            "GLIB_DOMAIN" => domain = f.value_str().unwrap_or("glib"),
            _ => {}
        }
    }
    let native = match level {
        glib::LogLevel::Error | glib::LogLevel::Critical => log::Level::Error,
        glib::LogLevel::Warning => log::Level::Warn,
        glib::LogLevel::Message | glib::LogLevel::Info => log::Level::Info,
        glib::LogLevel::Debug => log::Level::Trace, // glib's floor is Debug
    };
    if is_benign_gtk_startup_noise(&message) {
        // GTK-internal transients during early layout / surface realization — caused by
        // GTK/the theme, not app code, and self-corrected — so they are pure startup
        // noise at their native WARN/ERROR level. Demote to Debug so a default
        // `RUST_LOG=warn` doesn't surface them; `RUST_LOG=debug` still does.
        log::log!(target: domain, log::Level::Debug, "{message}");
        // …but the demotion is a DISPLAY decision, and the forensic record must not
        // inherit it (TDD 21.5). The only context either 2026-07-29 crash left behind
        // was exactly this class of message, so record it at its native level with
        // the threshold bypassed. This is the sole call site of that bypass, and the
        // demotion above is the sole reason one exists.
        forensics::record_demoted_diagnostic(native, domain, &message);
    } else {
        log::log!(target: domain, native, "{message}");
    }
    glib::LogWriterOutput::Handled
}

/// Initialise logging. Call **once**, first thing in `main()`, before any GTK call
/// so startup diagnostics are captured.
///
/// The Rust `log` backend stays the single sink and `RUST_LOG` still controls
/// stderr exactly as it did; what changed is that the registered logger is
/// [`crate::forensics`]'s wrapper around `env_logger`, which additionally records
/// every `info`-or-worse record into the persistent log and the breadcrumb ring the
/// crash handler reads. `env_logger`'s own `init()` is therefore not used — it
/// registers itself, and there can be only one global logger.
pub(crate) fn init() {
    let inner = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .format_timestamp_millis()
        .build();
    // Records reach a logger only if the global max level admits them, so this must
    // be at least the recording threshold — `RUST_LOG=warn` would otherwise filter
    // out the very lifecycle records the ring exists to hold.
    let max_level = forensics::sink::ForensicSink::recording_filter(inner.filter());
    let sink = forensics::install(inner);
    if log::set_logger(sink).is_ok() {
        log::set_max_level(max_level);
    }

    // Funnel all glib/GTK output into that sink. The writer func is the single
    // chokepoint and can be set exactly once (it panics on a second call), so this
    // must run once. NEVER also register `glib::GlibLogger`: it bridges the opposite
    // direction (log → glib), and combining the two recurses to a stack overflow
    // (ScrAP-18). The crash-forensics kit adds no glib sink of its own for the same
    // reason — it sits behind the Rust `log` facade, downstream of this bridge, so
    // GTK diagnostics become breadcrumbs by flowing through the one existing route.
    glib::log_set_writer_func(forward);

    // Announce a report left by a previous crash. After the logger is registered,
    // because the announcement is itself a log record (TDD 21.9).
    forensics::announce_previous_crash();
}

#[cfg(test)]
mod tests {
    use super::is_benign_gtk_startup_noise;

    #[test]
    fn demotes_only_the_known_benign_gtk_startup_transients() {
        // The three pinned transients demote to Debug (kept off a default startup log).
        assert!(is_benign_gtk_startup_noise(
            "GtkGizmo 0x1 (slider) reported min width -2, but sizes must be >= 0"
        ));
        assert!(is_benign_gtk_startup_noise(
            "gdk_monitor_get_geometry: assertion 'GDK_IS_MONITOR (monitor)' failed"
        ));
        assert!(is_benign_gtk_startup_noise(
            "Unable to connect to the accessibility bus at 'unix:path=/run/user/1000/at-spi/bus_0'"
        ));
        // A real app error must NOT be demoted.
        assert!(!is_benign_gtk_startup_noise(
            "Trying to snapshot ScribTableWidget without a current allocation"
        ));
        assert!(!is_benign_gtk_startup_noise("could not open file"));
    }
}
