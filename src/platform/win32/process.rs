//! A running pid's executable identity — the probe crash recovery needs and Windows
//! does not otherwise offer.
//!
//! Linux answers "what is this pid running" by reading `/proc/<pid>/comm`; macOS by
//! `libproc`'s `proc_pidpath`. Windows has neither, and the recovery scan's fallback
//! there deliberately answered "not live" — safe, but it meant two instances started at
//! once could each recover the other's in-progress snapshot into a tab of its own. This
//! module is the missing arm.
//!
//! Supplies a source only — never a liveness verdict of its own
//! (`window::swaprecovery::owner_is_live` decides what a name match means), matching
//! `platform::mac::process`.
//!
//! # Why this is not a transcription of the macOS probe
//!
//! **A handle to a dead process is still a valid handle.** This is the one place where
//! the Windows shape genuinely inverts the unix one, and getting it wrong produces a
//! false *"live"* — the single outcome the whole recovery feature exists to prevent,
//! because an instance that believes an owner is alive skips the recovery and silently
//! abandons the user's work. On Linux `/proc/<pid>` disappears when the process is
//! reaped. On Windows the *process object* outlives termination for as long as anyone
//! holds a handle to it, and `OpenProcess` on that pid **succeeds**. Measured here:
//!
//! | after the process was killed, with a handle still held | result |
//! |---|---|
//! | `OpenProcess(…)` | **succeeds**, `GetLastError() == 0` |
//! | `WaitForSingleObject(h, 0)` | `WAIT_OBJECT_0` — signalled, i.e. exited |
//! | `QueryFullProcessImageNameW(h, …)` | fails, `ERROR_GEN_FAILURE` (31) |
//! | `OpenProcess(…)` once every handle is released | fails, error 87 |
//!
//! That is not a contrived state for *this* feature in particular: recovery runs after a
//! crash, and Windows Error Reporting holds a handle to a just-crashed process while its
//! report is collected. The modal case is a live handle to a dead process.
//!
//! So existence is never taken as liveness. Note the third row means the name query
//! alone would in fact have discriminated — but that behaviour is **undocumented**, and
//! "does the user lose their unsaved work" is not a question to answer from an
//! observation that Microsoft never promised. [`WaitForSingleObject`] is documented,
//! costs one call, and makes correctness independent of that row.
//!
//! **`GetExitCodeProcess` is the wrong tool and was rejected on measurement.** It
//! reports `STILL_ACTIVE` (259) for a running process — so a process that genuinely
//! exits with code 259 is indistinguishable from one that is still running. Confirmed
//! observable here rather than taken from the documentation's warning.
//!
//! **`PROCESS_QUERY_LIMITED_INFORMATION`, not `PROCESS_QUERY_INFORMATION`.** The limited
//! right is the one that succeeds across integrity levels and for processes owned by
//! another user; the full right is refused there. Both fold to "cannot tell" and so to
//! "not live", which is safe — but the wrong constant would make the probe useless in
//! exactly the elevated and second-user cases it exists to cover.
//!
//! **Pid reuse is a residual, and it is worse here than on unix.** Windows recycles pids
//! from a small pool far more eagerly than Linux's near-monotonic allocator, so "this
//! pid is a different process now" is a likelier event. The name check absorbs the
//! ordinary case (the pid is now `notepad.exe`, so: not live). What it cannot absorb is
//! a reused pid that happens to be running *another* Scribobulate — that reads as live
//! and the snapshot is skipped. The hole is not introduced here: `/proc/<pid>/comm` and
//! `proc_pidpath` have exactly the same one, since a name is not an identity. Closing it
//! would mean recording the owner's process start time in [`crate::swapfile::SwapHeader`]
//! alongside `owner_pid` and comparing both — a schema change affecting all three
//! platforms, not a Windows patch.

use std::ffi::c_void;

use super::CloseHandle;

/// `PROCESS_QUERY_LIMITED_INFORMATION` — see the module doc for why not the full right.
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x0000_1000;
/// `SYNCHRONIZE`, required to wait on the handle at all. Without it the liveness gate
/// below cannot run and a dead process reads as live.
const SYNCHRONIZE: u32 = 0x0010_0000;
/// `WAIT_TIMEOUT` — the wait expired, i.e. the process object is **not** signalled, i.e.
/// it is still running. Any other answer means exited or unknowable.
const WAIT_TIMEOUT: u32 = 258;

/// Windows' long-path ceiling in UTF-16 code units. `MAX_PATH` is deliberately not used:
/// the state directory can sit well past 260 characters, and a truncating read would
/// answer `None` for a genuinely live instance.
const MAX_IMAGE_PATH: usize = 32768;

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit_handle: i32, pid: u32) -> *mut c_void;
    fn QueryFullProcessImageNameW(
        process: *mut c_void,
        flags: u32,
        buffer: *mut u16,
        size: *mut u32,
    ) -> i32;
    fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
}

/// The **running** `pid`'s executable basename, or `None` if `pid` is not running, or
/// this process cannot inspect it. Both fold into "unknown" at every call site.
///
/// The basename carries its extension, so this answers `scribobulate.exe` where the
/// macOS twin answers `scribobulate`. Comparison is the caller's business and must be
/// case-insensitive — measured, a stock `ping` reports
/// `C:\Windows\System32\PING.EXE`, so casing follows the filesystem rather than any
/// convention we could rely on.
pub(crate) fn executable_name(pid: u32) -> Option<String> {
    // SAFETY: no pointers in; the returned handle is either null or owned by us, and is
    // released on every path below.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        // Nonexistent pid, or access denied. Both are "cannot tell", which the caller
        // reads as not live.
        return None;
    }
    let name = running_image_name(handle);
    // SAFETY: `handle` came from the successful `OpenProcess` above and nothing reads it
    // after this. Split from the body precisely so there is one release on one path.
    unsafe { CloseHandle(handle) };
    name
}

/// The image basename behind an already-open process handle, gated on the process still
/// running. Separated from [`executable_name`] so the handle has exactly one owner and
/// exactly one release, rather than a `CloseHandle` on each early return.
fn running_image_name(handle: *mut c_void) -> Option<String> {
    // The liveness gate, and it comes FIRST — a terminated process can still answer an
    // `OpenProcess`, so anything after this point would otherwise be describing a
    // process that no longer exists.
    // SAFETY: `handle` is a live process handle opened with SYNCHRONIZE; a zero timeout
    // never blocks.
    if unsafe { WaitForSingleObject(handle, 0) } != WAIT_TIMEOUT {
        return None;
    }

    let mut buffer = vec![0u16; MAX_IMAGE_PATH];
    let mut size = buffer.len() as u32;
    // SAFETY: `buffer` holds `size` UTF-16 units and outlives the call; the API writes at
    // most that many and updates `size` with how many it actually wrote.
    let ok = unsafe {
        QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), std::ptr::addr_of_mut!(size))
    };
    if ok == 0 {
        return None;
    }
    // `size` is the length written, excluding the terminator — so this never reads past
    // what the call itself just initialised.
    let path = String::from_utf16_lossy(&buffer[..size as usize]);
    std::path::Path::new(&path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::{
        executable_name, OpenProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION,
        SYNCHRONIZE, WAIT_TIMEOUT,
    };

    // A DISCLOSURE ABOUT WHAT THESE TESTS DO NOT COVER, recorded because a passing suite
    // would otherwise imply the opposite.
    //
    // Deleting the `WaitForSingleObject` gate from `running_image_name` leaves every test
    // in this crate green. That was mutation-tested, not assumed. The reason is the third
    // row of the module doc's table: `QueryFullProcessImageNameW` *also* refuses for a
    // terminated process, so on this Windows build it independently produces the same
    // `None` and masks the gate's removal.
    //
    // The gate stays anyway, and the survival is not an argument for dropping it — it is
    // the argument FOR it. That row is undocumented behaviour, the gate's row is
    // documented, and what rests on the answer is whether a user's unsaved work is
    // silently discarded. What cannot be done is write a test that distinguishes them,
    // because doing so would require a dead process whose image name still resolves, and
    // no such state is reachable here.
    //
    // `waiting_on_the_handle_discriminates_a_running_process_from_a_dead_one` below is
    // the reachable half: it pins the primitive the gate rests on, so if a future Windows
    // stops signalling terminated process objects the failure is loud and local rather
    // than a silent false "live" in recovery.

    /// Something stock, short-lived and quiet. `/bin/sleep` has no drop-in here:
    /// `timeout.exe` refuses outright when stdin is redirected, which is exactly what
    /// `Command` does to it, so the classic `ping -n` idiom is used instead and its
    /// output discarded.
    fn spawn_ping(seconds: u32) -> std::process::Child {
        let ping = std::path::Path::new(&std::env::var("SystemRoot").unwrap_or_default())
            .join("System32")
            .join("ping.exe");
        std::process::Command::new(ping)
            .args(["-n", &seconds.to_string(), "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn ping.exe")
    }

    #[test]
    fn a_live_process_resolves_to_its_own_executable_basename() {
        let mut child = spawn_ping(30);
        let name = executable_name(child.id());
        let _ = child.kill();
        let _ = child.wait();

        let name = name.expect("a running process must resolve to a name");
        assert!(
            name.eq_ignore_ascii_case("ping.exe"),
            "expected ping.exe (any casing), got {name:?}",
        );
    }

    /// **The test no other platform can write.** `Child` owns the process handle on
    /// Windows and — unlike a unix `wait()`, which reaps the pid out of existence —
    /// holds it open past `wait()` until it is dropped. So at the assertion below the
    /// pid is dead *and still openable*, which is the precise state that would make a
    /// naive existence check report a false "live" and abandon the user's work.
    ///
    /// The first assertion is not decoration: it pins that this test is exercising the
    /// dead-but-openable path rather than trivially passing because the pid was already
    /// gone. If a future std release closes the handle in `wait()`, this fails loudly
    /// instead of quietly becoming vacuous.
    #[test]
    fn a_terminated_process_is_not_live_even_though_its_pid_still_opens() {
        let mut child = spawn_ping(30);
        let pid = child.id();
        child.kill().expect("kill the child");
        child.wait().expect("collect the child's status");

        // SAFETY: no pointers in; the handle, if any, is closed immediately below.
        let handle =
            unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, 0, pid) };
        assert!(
            !handle.is_null(),
            "precondition: `child` still holds a handle, so the dead pid must still open \
             — without that this test proves nothing",
        );
        // SAFETY: valid handle from the call above; nothing reads it afterwards.
        unsafe { super::CloseHandle(handle) };

        assert_eq!(
            executable_name(pid),
            None,
            "a terminated process must not resolve to a name, however openable its pid is",
        );
    }

    /// Pins the primitive the liveness gate rests on, across the transition, on **one**
    /// handle: the same `HANDLE` must answer `WAIT_TIMEOUT` while its process runs and
    /// something else once it does not. Two separate handles would not prove it, since a
    /// second `OpenProcess` could plausibly be the thing that changed.
    ///
    /// See the disclosure above for why this exists as its own test rather than being
    /// covered by the recovery-facing ones.
    #[test]
    fn waiting_on_the_handle_discriminates_a_running_process_from_a_dead_one() {
        let mut child = spawn_ping(30);
        // SAFETY: no pointers in; the handle is closed at the end of this test.
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE,
                0,
                child.id(),
            )
        };
        assert!(!handle.is_null(), "a running pid must open");

        // SAFETY: live handle opened with SYNCHRONIZE; a zero timeout never blocks.
        assert_eq!(
            unsafe { WaitForSingleObject(handle, 0) },
            WAIT_TIMEOUT,
            "a running process object must NOT be signalled",
        );

        child.kill().expect("kill the child");
        child.wait().expect("collect the child's status");

        // SAFETY: as above — the handle outlives the process it refers to, which is the
        // whole point.
        assert_ne!(
            unsafe { WaitForSingleObject(handle, 0) },
            WAIT_TIMEOUT,
            "a terminated process object must be signalled — the gate is inert without it",
        );

        // SAFETY: valid handle from the call above; nothing reads it afterwards.
        unsafe { super::CloseHandle(handle) };
    }

    /// And once every handle is gone the pid is simply absent — the ordinary case, kept
    /// so the two failure routes (`OpenProcess` refuses / the liveness gate refuses) are
    /// both covered rather than only whichever one happens to fire first.
    #[test]
    fn a_fully_released_pid_resolves_to_nothing() {
        let mut child = spawn_ping(30);
        let pid = child.id();
        child.kill().expect("kill the child");
        child.wait().expect("collect the child's status");
        drop(child);

        assert_eq!(executable_name(pid), None);
    }
}
