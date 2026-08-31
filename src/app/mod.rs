//! Application-level module: command descriptor tables, menu mnemonics, the
//! per-window menubar builder, file-opening/live-reload wiring, and the
//! `GApplication` setup. Split out of the former monolithic `app.rs` so each
//! concern lives in a focused, independently-reviewable file (POLICY.md code-style
//! 500-line guidance). The crate-level API other modules already depend on
//! (`crate::app::X`) is re-exported below unchanged, so the split is internal.

mod appactions;
mod commands;
mod menubar;
mod mnemonics;
mod open;
mod openbatch;
mod setup;
mod shortcuts;

pub(crate) use commands::{
    accel_hint, inline_accel, inline_cmd, tooltip_with_accel, FmtCmd, EDIT_CMDS, FILE_CMDS,
    FORMAT_CMDS, INLINE_ACCEL_CMDS, TBTN_SECTION_IDS, VIEW_CMDS, WELCOME,
};
pub(crate) use menubar::{
    build_menubar, build_reading_theme_toolbar_menu, defer_live_menu_mutation,
    update_format_menu_labels,
};
pub(crate) use mnemonics::{access_markup, access_shortcut, escape_mnemonic, mnem};
pub(crate) use open::{
    attach_file_backing, dialog_dir_for, find_open_tab_for_path, focus_tab, remember_dialog_dir,
    LAST_DIALOG_DIR,
};
pub(crate) use setup::accelerator_bindings;
/// The whole binding set for an explicitly named platform — the pure enumeration
/// `accel`'s cross-platform collision guard checks. Test-only: production code
/// wants [`accelerator_bindings`], which asks for the host.
#[cfg(test)]
pub(crate) use setup::accelerator_bindings_for;
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) use setup::register_accelerators;
pub(crate) use setup::setup_app;
pub(crate) use setup::{re_render_all_windows, reload_theme_css};
pub(crate) use shortcuts::make_shortcuts_window;

/// The `--new-instance` / `-n` decision, and the argv it leaves behind.
///
/// Pure, and extracted for that reason: it is the whole of a decision with a
/// recorded past failure (ScrAP-17 — a uniqueness flag parsed after
/// `g_application_register()` is parsed in the wrong process, so it is forwarded and
/// never spawns anything), and it sat inline in `run()`, which the coverage gate
/// cannot see. The caller does the two GTK-shaped things — set `NON_UNIQUE`, hand the
/// remaining arguments to `run_with_args` — and takes no decision of its own.
///
/// Both spellings are stripped whether or not either was found, so the arguments that
/// reach `HANDLES_OPEN` are file paths and nothing else.
pub(crate) fn new_instance_argv(args: Vec<String>) -> (bool, Vec<String>) {
    let is_flag = |a: &String| a == "--new-instance" || a == "-n";
    let force_new = args.iter().any(is_flag);
    let mut rest = args;
    rest.retain(|a| !is_flag(a));
    (force_new, rest)
}

/// The marker `--probe-startup` prints. **A CONTRACT WITH THE macOS PACKAGING GATE** —
/// `packaging/macos/verify-selfcontained.sh` greps for exactly this text, so it is a
/// published interface and not a log line. Change it and that gate goes red.
///
/// Deliberately ASCII, deliberately not routed through the logger, and deliberately not
/// translated. It replaces a grep for GLib's `Unknown option`, which was none of those
/// things: that string belongs to GLib's message catalogue and is translated, so the gate
/// it backed passed in English and FAILED on a German machine against the same bundle —
/// measured across four locales. A gate whose verdict depends on the tester's locale is
/// not a gate.
pub(crate) const STARTUP_PROBE_MARKER: &str = "scribobulate: startup-probe ok";

/// Is this a `--probe-startup` invocation?
///
/// WHAT REACHING THIS PROVES, which is the whole reason the flag exists: dyld binds every
/// `LC_LOAD_DYLIB` in the graph BEFORE `main()` runs, so a process that gets far enough to
/// answer this question has already resolved its entire library closure. The macOS
/// packaging gate uses that: it launches the bundled binary with the Homebrew prefix made
/// unreadable, and a bundle that still depends on Homebrew dies in dyld without ever
/// reaching here. Silence is a failure; the marker is the pass.
///
/// WHAT IT DOES NOT PROVE: anything `dlopen`ed later — gdk-pixbuf loaders, GIO modules,
/// GSettings schemas — is not in the static graph and is not exercised by this. Those need
/// their own assertions, and a probe that returns 0 must not be read as "the bundle is
/// complete".
///
/// Pure, and unit-tested here rather than at the call site, for the reason
/// `new_instance_argv` above is: the coverage gate cannot reach `lib.rs`.
pub(crate) fn is_startup_probe(args: &[String]) -> bool {
    args.iter().any(|a| a == "--probe-startup")
}

#[cfg(test)]
mod startup_probe_tests {
    use super::{is_startup_probe, STARTUP_PROBE_MARKER};

    fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn the_flag_is_recognised_only_in_its_exact_spelling() {
        assert!(is_startup_probe(&argv(&[
            "scribobulate",
            "--probe-startup"
        ])));
        assert!(is_startup_probe(&argv(&[
            "scribobulate",
            "a.md",
            "--probe-startup"
        ])));
        // NEAR MISSES MUST NOT TRIGGER IT. A document legitimately named
        // `--probe-startup.md`, or a prefix of the flag, would otherwise make an
        // ordinary launch exit silently instead of opening anything.
        for near in ["--probe", "--probe-startup.md", "probe-startup", "-p"] {
            assert!(
                !is_startup_probe(&argv(&["scribobulate", near])),
                "{near} must not be read as the probe flag"
            );
        }
        assert!(!is_startup_probe(&argv(&["scribobulate", "a.md"])));
    }

    /// The marker is an interface, so its SHAPE is asserted, not just its presence.
    /// A gate greps for it on one line; an empty or multi-line value would break that
    /// silently on the packaging seat rather than here.
    #[test]
    fn the_marker_is_a_single_nonempty_ascii_line() {
        assert!(!STARTUP_PROBE_MARKER.is_empty());
        assert!(!STARTUP_PROBE_MARKER.contains('\n'));
        assert!(STARTUP_PROBE_MARKER.is_ascii());
    }
}

#[cfg(test)]
mod new_instance_tests {
    use super::new_instance_argv;

    fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn both_spellings_are_recognised_and_stripped() {
        for flag in ["--new-instance", "-n"] {
            let (force_new, rest) = new_instance_argv(argv(&["scribobulate", flag, "a.md"]));
            assert!(force_new, "{flag} must force a new instance");
            assert_eq!(rest, argv(&["scribobulate", "a.md"]));
        }
    }

    #[test]
    fn an_absent_flag_leaves_the_arguments_untouched() {
        let (force_new, rest) = new_instance_argv(argv(&["scribobulate", "a.md", "b.md"]));
        assert!(!force_new);
        assert_eq!(rest, argv(&["scribobulate", "a.md", "b.md"]));
    }

    /// A repeat, and a filename that merely CONTAINS a spelling, are both handled —
    /// the match is on the whole argument, so `-notes.md` is a file.
    #[test]
    fn matching_is_on_the_whole_argument_and_survives_repeats() {
        let (force_new, rest) = new_instance_argv(argv(&[
            "scribobulate",
            "-n",
            "-notes.md",
            "--new-instance",
            "--new-instance-x",
        ]));
        assert!(force_new);
        assert_eq!(
            rest,
            argv(&["scribobulate", "-notes.md", "--new-instance-x"])
        );
    }
}
