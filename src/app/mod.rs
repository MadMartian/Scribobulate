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
