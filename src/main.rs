//! Process entry point for Scribobulate. Deliberately three lines of code.
//!
//! Everything — the module tree, startup ordering, the `GApplication` — lives in
//! `src/lib.rs` and runs from [`scribobulate::run`]. This file exists only to be a
//! `[[bin]]` target, because a binary crate exposes no importable surface and so
//! cannot be reached by an integration test. Keeping it empty is the point: any
//! logic added here becomes untestable by construction. See `src/lib.rs`'s module
//! docs.
//!
//! The one thing that must stay here is the crate-level attribute below: a
//! subsystem declaration applies to the crate being *linked into an executable*,
//! so it has no effect from the library.

// Windows release builds link as a GUI ("windows") subsystem binary rather than a
// console one. Without this, launching the installed app allocates a console window
// that sits behind the real window for the whole session — and because that console
// owns the process, closing it kills the app.
//
// Gated on `not(debug_assertions)` deliberately: a debug build keeps its console, so
// `RUST_LOG` output and any panic message still reach the terminal during development.
// A release build has no console to write to, which is the correct trade for a shipped
// desktop app — diagnostics there come from `RUST_LOG` redirected to a file.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() -> gtk::glib::ExitCode {
    scribobulate::run()
}
