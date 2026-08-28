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
