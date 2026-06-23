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
