//! A running pid's executable identity, via `libproc`'s `proc_pidpath`.
//!
//! This is the officially supported, ABI-stable way to ask "what is this pid
//! running" on macOS. The obvious alternative — `sysctl(CTL_KERN, KERN_PROC,
//! KERN_PROC_PID, pid)` filling a `struct kinfo_proc` — is what `ps`/Activity
//! Monitor use internally, but Apple documents that struct's layout as unstable
//! across releases (which is also why the `libc` crate, unlike its
//! FreeBSD/NetBSD/OpenBSD siblings, defines no `kinfo_proc` for
//! `target_os = "macos"` at all — there is nothing safe to bind). `proc_pidpath`
//! avoids the whole question: it is a stable `libproc.h` entry point, exported by
//! `libSystem`, which every macOS binary already links, so no extra
//! `#[link(...)]` is needed here (contrast `appearance.rs`'s CoreFoundation
//! import, a separate framework).
//!
//! Supplies a source only — never a liveness verdict of its own (`window::swaprecovery::owner_is_live` decides what a name match means).

/// The running `pid`'s executable basename, or `None` if `pid` does not exist, or
/// this process cannot inspect it. Both fold into "unknown" at every call site.
pub(crate) fn executable_name(pid: u32) -> Option<String> {
    // `libproc.h`'s own cap: `PROC_PIDPATHINFO_MAXSIZE` = 4 * `MAXPATHLEN`.
    const PROC_PIDPATHINFO_MAXSIZE: usize = 4 * 1024;

    unsafe extern "C" {
        fn proc_pidpath(
            pid: libc::c_int,
            buffer: *mut libc::c_void,
            buffersize: u32,
        ) -> libc::c_int;
    }

    let mut buf = [0u8; PROC_PIDPATHINFO_MAXSIZE];
    // SAFETY: `buf` is a valid, correctly-sized stack buffer that outlives the call;
    // `proc_pidpath` writes at most `buffersize` bytes into it and returns the number
    // of bytes actually written (<= 0 on failure), so the slice below never reads
    // past what the call itself just initialised.
    let len = unsafe {
        proc_pidpath(
            pid as libc::c_int,
            buf.as_mut_ptr().cast(),
            buf.len() as u32,
        )
    };
    if len <= 0 {
        return None;
    }
    let path = std::str::from_utf8(&buf[..len as usize]).ok()?;
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string)
}

/// This process's physical footprint in bytes, via `proc_pid_rusage(RUSAGE_INFO_V2)`,
/// for the memory gates (`memgate::footprint`).
///
/// `RUSAGE_INFO_V2` is the earliest flavour that carries `ri_phys_footprint`.
///
/// ⚠ This number is a high-water of pages the zone still holds, not "bytes currently
/// referenced". macOS malloc keeps freed pages: a 256 MB allocation dropped moved the
/// reading by nothing. Never write a single-shot "allocate, free, assert this came
/// back" against it.
#[cfg(test)]
pub(crate) fn phys_footprint_bytes() -> Option<u64> {
    // SAFETY: `rusage_info_v2` is the buffer `RUSAGE_INFO_V2` writes; a zeroed struct is
    // a valid empty starting point, and `getpid` is this process.
    unsafe {
        let mut info: libc::rusage_info_v2 = std::mem::zeroed();
        let rc = libc::proc_pid_rusage(
            libc::getpid(),
            libc::RUSAGE_INFO_V2,
            std::ptr::addr_of_mut!(info) as *mut libc::rusage_info_t,
        );
        (rc == 0).then_some(info.ri_phys_footprint)
    }
}

/// What this login session says about the screen, for the memory gates: `None` when the
/// session dictionary could not be read at all, `Some(None)` when it was read and carries
/// no `CGSSessionScreenIsLocked` key (an UNLOCKED screen has no such key, it is not
/// `false`), `Some(Some(flag))` when the key is present.
///
/// The three are kept apart because "absent" and "the read failed" mean opposite things and
/// a gate that folds them together either skips on an unlocked screen or measures on a
/// locked one.
#[cfg(test)]
pub(crate) fn session_screen_lock() -> Option<Option<bool>> {
    use std::ffi::{c_char, c_void};

    // `kCFStringEncodingUTF8`.
    const UTF8: u32 = 0x0800_0100;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {}
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGSessionCopyCurrentDictionary() -> *const c_void;
        fn CFStringCreateWithCString(
            alloc: *const c_void,
            c_str: *const c_char,
            encoding: u32,
        ) -> *const c_void;
        fn CFDictionaryGetValue(dict: *const c_void, key: *const c_void) -> *const c_void;
        fn CFBooleanGetValue(boolean: *const c_void) -> u8;
        fn CFRelease(object: *const c_void);
    }

    // SAFETY: each CF object created here is released exactly once, in this function, and
    // none escapes it; `CFDictionaryGetValue` returns a borrowed value that is read before
    // the dictionary is released; the key is a NUL-terminated literal.
    unsafe {
        let dict = CGSessionCopyCurrentDictionary();
        if dict.is_null() {
            return None;
        }
        let key =
            CFStringCreateWithCString(std::ptr::null(), c"CGSSessionScreenIsLocked".as_ptr(), UTF8);
        if key.is_null() {
            CFRelease(dict);
            return None;
        }
        let value = CFDictionaryGetValue(dict, key);
        let flag = (!value.is_null()).then(|| CFBooleanGetValue(value) != 0);
        CFRelease(key);
        CFRelease(dict);
        Some(flag)
    }
}

/// The address of `symbol` in the librsvg dyld has already loaded into this process, or
/// `None` when none is. Found in dyld's image list, not by name lookup, so this names
/// the decoder in use (Homebrew's, or a bundle's staged copy), never one merely
/// installed; `RTLD_NOLOAD` keeps the lookup from loading anything.
#[cfg(test)]
pub(crate) fn loaded_librsvg_symbol(symbol: &std::ffi::CStr) -> Option<*const libc::c_void> {
    use std::os::unix::ffi::OsStrExt;
    let image = super::bundle::loaded_images()
        .into_iter()
        .find(|p| p.file_name().is_some_and(|n| n == "librsvg-2.2.dylib"))?;
    let path = std::ffi::CString::new(image.as_os_str().as_bytes()).ok()?;
    // SAFETY: dlopen with RTLD_NOLOAD returns a handle to an already-loaded image or
    // NULL; dlsym reads that image's symbol table; dlclose drops only the reference
    // this call took, and the loader that loaded librsvg keeps it mapped, so the
    // address stays valid for the life of the process.
    unsafe {
        let handle = libc::dlopen(path.as_ptr(), libc::RTLD_LAZY | libc::RTLD_NOLOAD);
        if handle.is_null() {
            return None;
        }
        let address = libc::dlsym(handle, symbol.as_ptr());
        libc::dlclose(handle);
        (!address.is_null()).then_some(address.cast_const())
    }
}

#[cfg(test)]
mod tests {
    use super::executable_name;

    #[test]
    fn a_pid_with_no_running_process_resolves_to_nothing() {
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("0")
            .spawn()
            .expect("spawn /bin/sleep");
        let pid = child.id();
        child.wait().expect("reap the child");
        assert_eq!(executable_name(pid), None);
    }

    #[test]
    fn a_live_process_resolves_to_its_own_executable_basename() {
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("5")
            .spawn()
            .expect("spawn /bin/sleep");
        let pid = child.id();
        assert_eq!(executable_name(pid).as_deref(), Some("sleep"));
        let _ = child.kill();
        let _ = child.wait();
    }
}
