//! The release build is a GUI-subsystem program (`src/main.rs`), so Windows gives it no
//! console: a switch answered in a terminal (`--help`, `--version`, a refused
//! `--instance` name) printed into nothing, and the prompt came back blank (TDD 8.8).
//! Redirected output was never affected, because a redirect hands the process real
//! standard handles.
//!
//! [`attach_parent_console`] joins the console of the process that launched this one,
//! and only when a standard handle is unset — so a redirect or pipe is left exactly as
//! it was. It is called only on the paths that print and then exit: a process that
//! stays running must not hold the terminal's console, or closing that terminal would
//! end the app with it.

use std::ffi::c_void;

/// `AttachConsole`'s "the parent process" argument: `(DWORD)-1`.
const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
/// `STD_OUTPUT_HANDLE` and `STD_ERROR_HANDLE`: `(DWORD)-11` and `(DWORD)-12`.
const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
const STD_ERROR_HANDLE: u32 = -12i32 as u32;

#[link(name = "kernel32")]
extern "system" {
    fn AttachConsole(process_id: u32) -> i32;
    fn GetStdHandle(which: u32) -> *mut c_void;
}

/// Is this standard handle absent — null (a GUI process's default) or
/// `INVALID_HANDLE_VALUE`?
fn unset(handle: *mut c_void) -> bool {
    handle.is_null() || handle as isize == -1
}

/// Join the launching terminal's console so what follows reaches it. Does nothing when
/// both standard handles are already set (redirected or piped), or when the parent has
/// no console (launched from Explorer): `AttachConsole` then fails, harmlessly.
pub(crate) fn attach_parent_console() {
    // SAFETY: both calls take plain integers and return a handle or a BOOL; neither
    // reads memory this process owns.
    unsafe {
        if unset(GetStdHandle(STD_OUTPUT_HANDLE)) || unset(GetStdHandle(STD_ERROR_HANDLE)) {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}
