//! The release build is a GUI-subsystem program (`src/main.rs`), so Windows gives it no
//! console: a switch answered in a terminal (`--help`, `--version`, a refused
//! `--instance` name) printed into nothing, and the prompt came back blank (TDD 8.8).
//! Redirected output was never affected, because a redirect hands the process real
//! standard handles.
//!
//! [`attach_parent_console`] joins the console of the process that launched this one
//! when a standard handle is unset, and puts back any handle that was set: attaching
//! resets all three standard handles to the console, so without that a redirected
//! stream (`--help > f.txt`, `2> e.txt`) went to the screen and its file stayed empty
//! (MEASURED on the `windows` seat, conhost and ConPTY). It is called only on the paths that print and then exit: a process that
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
    fn SetStdHandle(which: u32, handle: *mut c_void) -> i32;
}

/// Is this standard handle absent — null (a GUI process's default) or
/// `INVALID_HANDLE_VALUE`?
fn unset(handle: *mut c_void) -> bool {
    handle.is_null() || handle as isize == -1
}

/// Join the launching terminal's console so what follows reaches it, keeping every
/// standard handle that was already set (redirected or piped). Does nothing when both
/// are set, or when the parent has no console (launched from Explorer): `AttachConsole`
/// then fails, harmlessly.
pub(crate) fn attach_parent_console() {
    // SAFETY: every call takes plain integers or a handle this process was given, and
    // returns a handle or a BOOL; none reads memory this process owns.
    unsafe {
        let out = GetStdHandle(STD_OUTPUT_HANDLE);
        let err = GetStdHandle(STD_ERROR_HANDLE);
        if !unset(out) && !unset(err) {
            return;
        }
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return;
        }
        for (which, kept) in [(STD_OUTPUT_HANDLE, out), (STD_ERROR_HANDLE, err)] {
            if !unset(kept) {
                SetStdHandle(which, kept);
            }
        }
    }
}
