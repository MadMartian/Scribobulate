//! Application-level module: command descriptor tables, menu mnemonics, the
//! per-window menubar builder, file-opening/live-reload wiring, and the
//! `GApplication` setup. Split out of the former monolithic `app.rs` so each
//! concern lives in a focused, independently-reviewable file (POLICY.md code-style
//! file-size guidance). The crate-level API other modules already depend on
//! (`crate::app::X`) is re-exported below unchanged, so the split is internal.

mod appactions;
pub(crate) mod coldstart;
mod commands;
mod menubar;
mod mnemonics;
mod open;
mod openbatch;
#[cfg(all(test, feature = "gtk-integration-tests"))]
pub(crate) use openbatch::BATCHES_BUILT;
mod openselection;
mod setup;
mod shortcuts;

pub(crate) use commands::{
    accel_hint, inline_accel, inline_cmd, tooltip_with_accel, Cmd, FmtCmd, EDITOR_KEYS, EDIT_CMDS,
    FILE_CMDS, FORMAT_CMDS, INLINE_ACCEL_CMDS, TBTN_SECTION_IDS, VIEW_CMDS, WELCOME,
};
pub(crate) use commands::{
    toolbar_section_action, toolbar_toggles, ToolbarToggle, TOOLBAR_SHOW_ACTION,
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
/// The whole binding set for an explicitly named platform — the pure enumeration
/// `accel`'s cross-platform collision guard checks. Test-only: production code
/// wants `setup::accelerator_bindings`, which asks for the host.
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
/// recorded past failure (GTK4Rs/AP-17 — a uniqueness flag parsed after
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

/// Does argv carry an argument shaped like an option?
///
/// **This is the whole fix for a `--` switch being opened as a document.** On macOS the
/// single-instance substitute forwards its arguments to a running primary, which treats
/// what it receives as file paths — so `scribobulate --help` against a running instance
/// asked that instance to open a document named `--help`, and the launching process exited
/// 0 having printed nothing. The reporter saw a switch silently do nothing; the primary saw
/// a file it could not open. Linux never showed it, because GIO parses options before it
/// forwards, so the two platforms disagreed about what an argument even IS.
///
/// So: if anything option-shaped is present, the caller must NOT forward and must let
/// GOption answer in this process — where `--help` prints help and an unknown option is
/// refused, identically on every platform.
///
/// The rules are POSIX's, not ours: a bare `--` ends option parsing (everything after it is
/// a path, however it is spelled), and a lone `-` is a path, not an option. Both matter to
/// anyone whose document is genuinely called `--help`, who can still open it as
/// `scribobulate -- --help`.
///
/// Deliberately NOT `#[cfg(target_os = "macos")]` even though the handoff is: the decision is
/// about argv, not about a platform, and cfg-ing it would take its tests out of the build on
/// every other machine — which POLICY forbids for exactly the reason that a deleted test and
/// a passing one are indistinguishable. The allow is therefore scoped to the platforms with
/// no production caller, and the tests below still run everywhere.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn carries_option_argument(args: &[String]) -> bool {
    args.iter()
        .skip(1)
        .take_while(|a| a.as_str() != "--")
        .any(|a| a.starts_with('-') && a.as_str() != "-")
}

/// Is this a `--version` / `-V` invocation?
pub(crate) fn is_version_request(args: &[String]) -> bool {
    args.iter()
        .skip(1)
        .take_while(|a| a.as_str() != "--")
        .any(|a| a == "--version" || a == "-V")
}

/// What `--version` prints: the name a user typed and the version they have.
///
/// Pure so the format is asserted without running the binary, and taken from Cargo rather
/// than restated, so it cannot drift from the package it describes.
pub(crate) fn version_line() -> String {
    format!("scribobulate {}", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod option_argument_tests {
    use super::{carries_option_argument, is_version_request, version_line};

    fn argv(rest: &[&str]) -> Vec<String> {
        std::iter::once("scribobulate")
            .chain(rest.iter().copied())
            .map(String::from)
            .collect()
    }

    /// The reported defect: a switch must never look like a document.
    ///
    /// Mutation check: dropping the `starts_with('-')` test makes every one of these false.
    #[test]
    fn an_option_shaped_argument_is_recognised_as_one() {
        for a in [
            "--help",
            "--version",
            "--nonsense",
            "-h",
            "-n",
            "--gtk-debug=all",
        ] {
            assert!(carries_option_argument(&argv(&[a])), "{a} reads as a path");
        }
    }

    /// argv[0] is the program, not an argument — and a program invoked by a path beginning
    /// with a dash would otherwise make every launch look like an option.
    #[test]
    fn the_program_name_is_not_an_argument() {
        assert!(!carries_option_argument(&["-weird-name".into()]));
    }

    /// POSIX, and the escape hatch for a document genuinely named like a switch.
    ///
    /// Mutation check: removing the `take_while` makes both of these true, which would
    /// leave no way to open such a file at all.
    #[test]
    fn a_bare_double_dash_ends_option_parsing() {
        assert!(!carries_option_argument(&argv(&["--", "--help"])));
        assert!(!carries_option_argument(&argv(&["--", "-n"])));
        assert!(!is_version_request(&argv(&["--", "--version"])));
    }

    /// A lone `-` is a filename by convention, not an option.
    #[test]
    fn a_lone_dash_is_a_path() {
        assert!(!carries_option_argument(&argv(&["-"])));
    }

    #[test]
    fn ordinary_paths_carry_no_option() {
        assert!(!carries_option_argument(&argv(&["notes.md", "a/b.md"])));
        assert!(!carries_option_argument(&argv(&[])));
    }

    #[test]
    fn version_is_recognised_in_both_spellings_and_nowhere_else() {
        assert!(is_version_request(&argv(&["--version"])));
        assert!(is_version_request(&argv(&["-V"])));
        assert!(!is_version_request(&argv(&["--versionx"])));
        assert!(!is_version_request(&argv(&["-v"])), "-v is not --version");
        assert!(!is_version_request(&argv(&["notes.md"])));
    }

    /// The version string is taken from Cargo, never restated.
    #[test]
    fn the_version_line_names_the_package_version() {
        let line = version_line();
        assert!(line.starts_with("scribobulate "), "{line}");
        assert!(line.ends_with(env!("CARGO_PKG_VERSION")), "{line}");
    }

    /// Every option this binary answers itself must be option-shaped, or it would be routed
    /// to the primary as a filename instead of reaching the parser. GTK's own `--help*`
    /// family is deliberately absent: GOption owns those, and restating them here would be a
    /// second copy of somebody else's table.
    #[test]
    fn every_own_option_is_option_shaped() {
        for opt in ["--new-instance", "-n", "--probe-startup", "--version", "-V"] {
            assert!(
                carries_option_argument(&argv(&[opt])),
                "{opt} would never reach the parser"
            );
        }
    }
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
