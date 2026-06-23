//! The whole of the project's Win32 surface — one module boundary, four causes.
//!
//! **This is still "one place that may talk to Win32 directly" (POLICY).** It is a
//! directory rather than a single file only because the file crossed POLICY's 500-line
//! soft limit and kept going; the property the one-place rule protects is that every
//! call past GTK passes through *one* `#[cfg(windows)]`-gated module, declared once in
//! [`crate::platform`], and that is unchanged. `platform/mac/` is a directory for the
//! same reason and the two are now structurally symmetric. Nothing outside this
//! directory gained the ability to reach the OS, and every public name is re-exported
//! below so no caller can tell the split happened.
//!
//! **The children split by CAUSE, never by mechanism** — the organising principle
//! POLICY states for this module. There is deliberately no `dwmapi.rs` or
//! `advapi32.rs`: splitting on which DLL a call lands in would scatter one decision's
//! consequences across several files, which is the exact failure the rule names. Two
//! of these children call the same DLL and belong apart; one calls three and belongs
//! together.
//!
//! | Child | Cause |
//! |---|---|
//! | [`frame`] | The app takes the **native Win32 frame** (`GTK_CSD=0`), so DWM owns the caption and the OS owns the maximize button. Two GTK repairs, one decision. |
//! | [`appearance`] | GTK 4.22.4 does not read the Windows light/dark setting **at all**, so the desktop's lightness has no source here unless this supplies one. |
//! | [`privacy`] | `std::fs` cannot express an owner-only directory on Windows, so the state directory would carry whatever ACL it inherited. |
//! | [`process`] | Windows has no `/proc` and no `proc_pidpath`, so crash recovery cannot otherwise tell whether a snapshot's owner is still running. |
//!
//! Note that `frame` and `appearance` are two causes and not one, though they meet at
//! the caption. The caption request is a consequence of the native frame; the *source*
//! it reads is a consequence of GTK's missing Windows theme, and that source feeds the
//! editor, the toolbar and the sidebar as well. Both were derived from the same
//! investigation, which is why the original file held them together — but the caption
//! fix alone would have been inert, because with no source
//! [`desktop_is_dark`](crate::palette::desktop_is_dark) could never have returned true.
//! Keeping them apart is what stops that asymmetry being forgotten again.
//!
//! **The API is public, not a private reach.** `gdk_win32_surface_get_handle` is
//! documented GDK-Win32 API exported from the gtk-4 library, and everything else here is
//! public Win32. Nothing is a vtable hack. They are declared by hand rather than pulled
//! from `gdk4-win32`/`windows-sys` because a few `extern` blocks cost less than two
//! crates, and `gdk4-win32` tracks its own gdk4 version — a second gdk4 in the tree is a
//! worse trade than the FFI below.
//!
//! Gated at the module rather than internally (`#[cfg(windows)]` at its declaration in
//! [`crate::platform`], beside the macOS seam), matching how `workaround` is gated for
//! unix — a non-Windows build compiles none of it, so the `-D warnings` clippy gate
//! stays satisfiable with no `#[allow]`.

pub(crate) mod appearance;
pub(crate) mod frame;
pub(crate) mod privacy;
pub(crate) mod process;

// Re-exported so the split is invisible to callers: every one of these was
// `platform::win32::X` before the decomposition and still is. Moving a function between
// children is therefore a local edit, not a tree-wide rename — which is what keeps the
// cause-based boundaries above cheap enough to honour.
pub(crate) use appearance::track_system_dark_mode;
pub(crate) use frame::{sync_caption_theme, track_caption_theme, track_maximized_size};
pub(crate) use privacy::create_private_directory;
#[cfg(test)]
pub(crate) use privacy::directory_dacl_sddl;

use std::ffi::c_void;

// The two calls with no cause of their own: every child that touches a handle or reads a
// failure code needs them, so declaring them per-child would be four copies of the same
// two lines. Anything DLL-specific to one cause stays with that cause.
#[link(name = "kernel32")]
extern "system" {
    fn CloseHandle(handle: *mut c_void) -> i32;
    fn GetLastError() -> u32;
}

/// A UTF-16, NUL-terminated copy of `s`, as the `W` APIs require.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::wide;

    #[test]
    fn wide_nul_terminates() {
        // The `W` APIs read until NUL. A buffer without one is an out-of-bounds read in
        // someone else's code, so this is worth pinning even though the function is
        // three lines.
        assert_eq!(wide("Ab"), vec![b'A' as u16, b'b' as u16, 0]);
        assert_eq!(wide(""), vec![0]);
    }
}
