//! Which process owns the frontmost application, via
//! `-[NSWorkspace frontmostApplication]`.
//!
//! Test-only: a GTK test whose window never becomes active must tell "another
//! application holds the front" (the suite running behind a developer's other work)
//! from "this window failed to activate" (a regression). Only the first is a reason
//! to skip.

use std::ffi::{c_char, c_void};

// Declared by hand rather than by taking `objc2`, as `pasteboard.rs` does: three
// public, ABI-stable symbols.
type Id = *mut c_void;
type Sel = *const c_void;

#[link(name = "objc")]
unsafe extern "C" {
    fn objc_getClass(name: *const c_char) -> Id;
    fn sel_registerName(name: *const c_char) -> Sel;
    /// Untyped on purpose; see `fullscreen.rs`. Each use transmutes it to the exact
    /// signature of the selector it sends, which aarch64 requires.
    fn objc_msgSend();
    fn objc_autoreleasePoolPush() -> *mut c_void;
    fn objc_autoreleasePoolPop(pool: *mut c_void);
}

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}

/// The pid of the frontmost application, or `None` if the runtime does not answer
/// (no `NSWorkspace`, or no frontmost application at all).
pub(crate) fn frontmost_pid() -> Option<i32> {
    // SAFETY: the class and selector names are NUL-terminated literals, and the runtime
    // interns what it returns. Each `objc_msgSend` is called through a pointer carrying
    // the exact signature of the selector it sends: `+[NSWorkspace sharedWorkspace]`
    // and `-[NSWorkspace frontmostApplication]` (→ id) and
    // `-[NSRunningApplication processIdentifier]` (→ pid_t, an `i32`). Every object is
    // +0 and only read; the pool around the exchange frees anything AppKit autoreleases.
    unsafe {
        let class = objc_getClass(c"NSWorkspace".as_ptr());
        if class.is_null() {
            return None;
        }
        let shared = sel_registerName(c"sharedWorkspace".as_ptr());
        let frontmost = sel_registerName(c"frontmostApplication".as_ptr());
        let pid_of = sel_registerName(c"processIdentifier".as_ptr());
        let msg_id: extern "C" fn(Id, Sel) -> Id = std::mem::transmute(objc_msgSend as *const ());
        let msg_pid: extern "C" fn(Id, Sel) -> i32 = std::mem::transmute(objc_msgSend as *const ());

        let pool = objc_autoreleasePoolPush();
        let workspace = msg_id(class, shared);
        let app = if workspace.is_null() {
            std::ptr::null_mut()
        } else {
            msg_id(workspace, frontmost)
        };
        let pid = (!app.is_null()).then(|| msg_pid(app, pid_of));
        objc_autoreleasePoolPop(pool);
        pid
    }
}
