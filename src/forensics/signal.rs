//! The fatal-signal handler: what runs when the process is already dying.
//!
//! Hole 1 of the crash-forensics plan. A SIGSEGV writes **nothing** today — not the
//! signal, not the fault address, not a frame — and no core dump exists to make up
//! for it, because apport refuses an unpackaged executable (hole 2) on every Ubuntu
//! user's machine, not just this one. This module is the substitute: the process
//! reports on itself on the way down.
//!
//! # Everything here obeys async-signal-safety
//!
//! A SIGSEGV can land anywhere, including inside `malloc` while it holds its own
//! lock. Allocating or locking from the handler then **deadlocks** — the process
//! hangs instead of dying, which is strictly worse than the silence being fixed. So
//! the handler is built from `write`, `open`, `close`, `raise` and arithmetic:
//!
//! * the report path is a NUL-terminated byte string prepared at install time;
//! * the identity header is pre-rendered bytes, written verbatim;
//! * the only formatting is integers into a stack buffer ([`super::rawbuf`]);
//! * breadcrumbs come from a lock-free ring ([`super::ring`]);
//! * the backtrace uses `backtrace_symbols_fd`, which writes to a descriptor and
//!   never allocates (unlike `backtrace_symbols`, which does). **That argument
//!   covers the symbol-PRINTING half only.** `backtrace()` itself lazily
//!   `dlopen`s the unwinder on first call, which allocates and takes a loader
//!   lock; it is forced at install time by `prewarm_backtrace` so the handler
//!   never triggers it. Do not read the `backtrace_symbols_fd` argument as
//!   clearing the whole call — on other platforms the resolution half carries
//!   its own hazard that pre-warming does not address (ScrAP-214).
//!
//! # Two design choices that pay off exactly when things are worst
//!
//! **An alternate signal stack.** A stack-overflow SIGSEGV arrives with no usable
//! stack, so a handler on the normal stack faults again immediately and writes
//! nothing. `sigaltstack` gives the handler its own memory, so even that case gets
//! a report. It also replaces the alternate stack Rust's own stack-overflow handler
//! installed, which this handler necessarily displaces for SIGSEGV.
//!
//! **Re-raise, never swallow.** The handler restores the default disposition and
//! re-raises, so the process still dies from the signal it was killed by (TDD
//! 21.4): the shell's exit status, `journalctl`, and the kernel's own `segfault at`
//! line all keep saying what they said before. Nothing about the crash changes —
//! only the evidence left behind.

use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};

use libc::{c_int, c_void};

use super::rawbuf::RawBuf;
use super::ring::Ring;

/// The signals worth a report: the five ways this application can die without
/// running another line of its own code.
///
/// # Why `SIGTRAP` is here, when nothing in this process raises one deliberately
///
/// The other four are hardware faults or `abort()`. `SIGTRAP` is how a **GLib fatal
/// log message** dies on unix, and that is the death this application has actually
/// produced: `g_error` — GTK's `gtk_text_btree_line_number couldn't find line` among
/// them (ScrAP-258) — plus every warning promoted by `G_DEBUG=fatal-warnings`.
/// `_g_log_abort` assumes a debugger is attached on every non-Windows target and
/// executes `G_BREAKPOINT()` (`int $03` on x86) instead of `g_abort()`, so the process
/// dies of a breakpoint trap whose default disposition terminates it with no handler
/// having run. Enumerating only the four left that whole class silent (ScrAP-268).
///
/// MEASURED, GLib 2.72.4 on this machine: `g_error` and a `G_DEBUG=fatal-warnings`
/// warning both exit **133** (128 + `SIGTRAP`), while `g_assert_not_reached()` takes
/// `_g_log_abort`'s *other* branch and exits **134** (`SIGABRT`) — which is why the
/// assertion family was covered all along and the `g_error` family was not. The
/// end-to-end proof lives in
/// [`tests::a_glib_fatal_message_dies_by_a_signal_this_handler_takes`], which drives a
/// real `G_LOG_LEVEL_ERROR` through a real child process rather than asserting the
/// mechanism from prose.
///
/// **Taking it costs no debugging session** (MEASURED under gdb 12.1): a debugger
/// intercepts the trap through ptrace and reports `SIGTRAP … Pass to program: No`, so
/// it stops at the faulting frame exactly as before and this handler never runs there.
const FATAL_SIGNALS: [c_int; 5] = [
    libc::SIGSEGV,
    libc::SIGBUS,
    libc::SIGILL,
    libc::SIGABRT,
    libc::SIGTRAP,
];

/// Handler-readable state, published once at install time.
///
/// `AtomicPtr`/`AtomicUsize` rather than a `OnceLock`: reading a `OnceLock` in a
/// signal handler is not obviously safe, whereas an atomic load provably is.
static REPORT_PATH: AtomicPtr<i8> = AtomicPtr::new(std::ptr::null_mut());
static HEADER_PTR: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());
static HEADER_LEN: AtomicUsize = AtomicUsize::new(0);
static BREADCRUMBS: AtomicPtr<Ring> = AtomicPtr::new(std::ptr::null_mut());

/// Guards against a fault *inside* the handler turning into an endless loop of
/// faults. On re-entry the handler gives up immediately and lets the default
/// disposition finish the job — the partial report is already on disk, which is
/// what TDD 21.6 asks for.
static IN_HANDLER: AtomicBool = AtomicBool::new(false);

/// Install the handler. Called once, from [`super::install`].
///
/// `report_path` is the file the handler will create *if* it ever runs; nothing is
/// created now, so an ordinary run leaves no litter.
///
/// **There is deliberately no uninstall.** A production process wants this armed for
/// its whole life, and an uninstall would only add a window in which it can die
/// unreported. The one place that is wrong is a *test* process — libtest runs the
/// whole suite in one — so the tests below arm through `tests::ArmedHandler` (not a
/// doc link: it is `cfg(test)`, so one would dangle in an ordinary build), which
/// puts the dispositions and the alternate stack back. A test that calls this function
/// directly leaves the handler armed over every test after it; that is not a style
/// point, it is a measured failure (see that type's documentation).
pub(crate) fn install(report_path: &std::path::Path, header: &str, ring: &'static Ring) {
    // Leak all three deliberately: they must stay valid for the whole process
    // lifetime, and a handler that had to check whether its state was still alive
    // would be a handler with a race in it.
    let Ok(path) = std::ffi::CString::new(report_path.as_os_str().as_encoded_bytes()) else {
        // A NUL inside a path is unreachable on unix, but a crash handler that
        // panics during installation would be a self-inflicted outage.
        log::warn!("crash reports disabled: the report path contains a NUL byte");
        return;
    };
    REPORT_PATH.store(path.into_raw(), Ordering::Release);

    let header: &'static [u8] = Box::leak(header.as_bytes().to_vec().into_boxed_slice());
    HEADER_PTR.store(header.as_ptr().cast_mut(), Ordering::Release);
    HEADER_LEN.store(header.len(), Ordering::Release);
    BREADCRUMBS.store(std::ptr::from_ref(ring).cast_mut(), Ordering::Release);

    install_alternate_stack();
    prewarm_backtrace();

    for signal in FATAL_SIGNALS {
        // SAFETY: a zeroed `sigaction` is a valid one; `on_fatal` has the
        // `SA_SIGINFO` three-argument signature declared below, and every field is
        // initialised before the call.
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            // Via `*const ()`: casting a function item straight to an integer is a
            // lint (the item is not a pointer yet), and `sa_sigaction` is an integer.
            action.sa_sigaction = on_fatal as *const () as usize;
            action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(signal, &action, std::ptr::null_mut());
        }
    }
}

/// Force glibc's `backtrace` machinery to finish its one-time initialisation
/// **now**, on an ordinary thread, so the handler never triggers it (SEC-001, QA
/// round 3).
///
/// `backtrace()` on glibc resolves its unwinder lazily: the first call `dlopen`s
/// `libgcc_s.so`. That allocates and takes the dynamic loader's lock. Neither is
/// async-signal-safe, and inside a fatal handler on an already-dying process the
/// failure is a **deadlock** — strictly worse than the missing report the
/// backtrace was added to provide, because the process then hangs instead of
/// dying.
///
/// The module's own doc argues for `backtrace_symbols_fd` over
/// `backtrace_symbols` because the latter allocates. That argument is correct
/// and it is about a **different function**: it addresses the symbol-*printing*
/// half and says nothing about `backtrace()` itself. Reading it as settling the
/// safety of the whole call is how this survived review. (The doc has been
/// amended accordingly, and `ScrAP-214` records the same trap for the macOS arm,
/// where the hazard is in symbol *resolution* and pre-warming does NOT fix it.)
///
/// Pre-warming is preferred over dropping the backtrace because it keeps the
/// diagnostic and removes the hazard: after this call the handler's `backtrace()`
/// is arithmetic over an already-loaded unwinder. Discarding the result is the
/// point — the call is made for its side effect on the loader.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn prewarm_backtrace() {
    const PREWARM_FRAMES: usize = 8;
    let mut frames: [*mut c_void; PREWARM_FRAMES] = [std::ptr::null_mut(); PREWARM_FRAMES];
    // SAFETY: `backtrace` fills at most `PREWARM_FRAMES` entries of a live array.
    // Called from ordinary code at startup, where allocation and loader locks are
    // exactly as safe as any other library call.
    unsafe {
        libc::backtrace(frames.as_mut_ptr(), PREWARM_FRAMES as c_int);
    }
}

/// No-op where [`write_backtrace`] is the stub that calls nothing.
#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn prewarm_backtrace() {}

/// Give the handler its own stack, so a stack-overflow SIGSEGV can still be
/// reported. 64 KiB: enough for the handler's buffers with room to spare, and
/// comfortably above `MINSIGSTKSZ` on every supported platform.
///
/// # Scope, which is narrower than it looks (SEC-002, QA round 3)
///
/// `sigaltstack` is a **per-thread** setting on every supported platform — it is
/// not a process-wide one, and this is called exactly once, from [`install`], on
/// whichever thread installs the handler. Every other thread in the process —
/// GLib's worker pool, GIO's async I/O threads, anything a dependency spawns —
/// has **no** alternate stack, so a stack-overflow SIGSEGV on one of those still
/// faults again inside the handler and writes nothing.
///
/// That is a real limitation and it is recorded rather than hidden, because the
/// obvious reading of "we install an alternate signal stack" is that the
/// guarantee is process-wide, and it never was. The threads that matter here are
/// overwhelmingly not ours to hook: there is no portable way to run code at the
/// entry of a thread another library spawned. What IS in our control:
///
/// * a thread this crate spawns itself would need its own alternate stack. **That
///   remedy is not currently offered**: `install_alternate_stack` below is private to
///   this module, and deliberately stays private — MEASURED, the crate spawns no
///   production thread today (the single `thread::spawn` in `src/` is inside a
///   `#[cfg(test)]` module), so exporting it would add a `pub(crate)` item with no
///   caller. Adding a real thread therefore means changing THIS module, not calling
///   into it. Said explicitly because the previous wording instructed a reader to call
///   a private function — advice that reads as actionable and does not compile at the
///   site it is addressed to, with a doc-link that resolved to the very item it was
///   written on (QA round 5, L-6). Note also that "idempotent" would be true of the
///   *effect* and not of the *cost*: each call leaks a fresh 64 KiB, which is correct
///   per-thread and wrong per-call;
/// * the specific overflow this was most likely to meet — an unbounded recursion
///   over document structure — is now prevented at its source by
///   [`crate::limits::MAX_NEST_DEPTH`] rather than merely reported, which is the
///   better half of the fix and does not depend on which thread it happens on.
///
/// A future per-thread install would need a `pthread_key`-style hook or a wrapper
/// around every thread spawn; neither is worth its complexity while the main
/// thread — the one running every document operation — is covered.
fn install_alternate_stack() {
    const ALT_STACK_BYTES: usize = 64 * 1024;
    let stack: &'static mut [u8] = Box::leak(vec![0u8; ALT_STACK_BYTES].into_boxed_slice());
    // SAFETY: the stack is leaked, so it outlives every handler invocation, and
    // its size exceeds MINSIGSTKSZ.
    unsafe {
        let alt = libc::stack_t {
            ss_sp: stack.as_mut_ptr().cast::<c_void>(),
            ss_flags: 0,
            ss_size: ALT_STACK_BYTES,
        };
        libc::sigaltstack(&alt, std::ptr::null_mut());
    }
}

/// The handler itself. Every call it makes is on POSIX's async-signal-safe list.
extern "C" fn on_fatal(signal: c_int, info: *mut libc::siginfo_t, context: *mut c_void) {
    if IN_HANDLER.swap(true, Ordering::SeqCst) {
        // A second fault while reporting the first. Whatever reached the file is
        // what the reader gets; die now rather than loop.
        die(signal);
    }

    let fd = open_report();

    write_bytes(
        fd,
        b"=== scribobulate crash report ===\n\nkind: signal\ncrashed: ",
    );
    let mut stamp: RawBuf<STAMP_CAP> = RawBuf::new();
    if let Some(millis) = realtime_unix_millis() {
        let _ = super::timefmt::civil_from_unix_millis(millis).write_iso8601(&mut stamp);
    }
    write_bytes(fd, stamp.as_bytes());
    write_bytes(fd, b"\n");

    // The identity block, verbatim — rendered before anything went wrong.
    let header = HEADER_PTR.load(Ordering::Acquire);
    let header_len = HEADER_LEN.load(Ordering::Acquire);
    if !header.is_null() {
        // SAFETY: leaked at install time, never freed, length stored beside it.
        write_bytes(fd, unsafe {
            std::slice::from_raw_parts(header, header_len)
        });
    }

    write_fault(fd, signal, info, context);
    write_breadcrumbs(fd);
    write_backtrace(fd);
    write_module_map(fd);

    if fd != libc::STDERR_FILENO {
        // SAFETY: `fd` is a descriptor this handler opened.
        unsafe { libc::close(fd) };
    }
    die(signal);
}

/// Restore the default disposition and re-raise, so the process dies exactly as it
/// would have with no handler installed (TDD 21.4).
///
/// **The `pthread_sigmask` is load-bearing, not defensive.** Entering a handler
/// installed without `SA_NODEFER` adds that signal to the thread's blocked mask for
/// the handler's duration, so `raise()` here merely marks it *pending*: it is
/// delivered when the handler returns, which never happens because the next line
/// exits. Without the unblock the process therefore leaves by `_exit(128 + signal)`
/// — a **normal exit whose status is 139**, exactly the number a shell prints for a
/// segfault. Everything downstream then quietly disagrees with reality: `WIFSIGNALED`
/// is false, no `segfault at …` line reaches the kernel log, and any supervisor
/// distinguishing a crash from a clean exit sees a clean exit. The plan's whole
/// evidence trail is built on that kernel line, so a report that cost us the line
/// would be a bad trade. Caught only by driving a real signal through a forked child
/// (see this module's tests); every inspection of the code read fine. ScrAP-203.
fn die(signal: c_int) -> ! {
    // SAFETY: every call here is async-signal-safe; after `SIG_DFL` and the unblock,
    // the raise is fatal.
    unsafe {
        libc::signal(signal, libc::SIG_DFL);
        let mut unblock: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut unblock);
        libc::sigaddset(&mut unblock, signal);
        libc::pthread_sigmask(libc::SIG_UNBLOCK, &unblock, std::ptr::null_mut());
        libc::raise(signal);
        // Unreachable once the signal is unblocked and defaulted; kept so the
        // handler cannot fall through into undefined behaviour.
        libc::_exit(128 + signal);
    }
}

/// Capacity of the crash timestamp's stack buffer, **derived** from the format's own
/// length rather than chosen.
///
/// It was a literal `32` while `timefmt` asserted, in prose, that "the signal handler
/// sizes a stack buffer from `ISO8601_LEN`". It did not. The comment described a
/// coupling that did not exist, and nothing could contradict it — the claim names a
/// real module in English rather than by path, so neither a citation gate nor a grep
/// for `signal.rs` can reach it (QA round 4; a strictly worse variant of ScrAP-216,
/// which at least mis-names something greppable).
///
/// **The headroom is deliberate and the reason is measured.** Sizing this to exactly
/// `ISO8601_LEN` would make the prose true and introduce the very fault it warns
/// about: `{:04}` on the year is a MINIMUM width, not a maximum, so a machine whose
/// clock reads year 10000 renders 25 bytes and year 100000 renders 26 (measured, not
/// reasoned). `RawBuf` truncates silently in release — its `truncated` flag is
/// `#[cfg(test)]` — so the report would lose the tail of its timestamp exactly when
/// the clock is absurd, which is disproportionately when a machine is misbehaving.
/// The same lesson as this round's announcement fix: do not assume a sane clock.
///
/// So: derived, plus room for a year up to 12 digits.
///
/// **There is no compile-time check of this, and the one that used to sit here was a
/// third round of the same mistake** (QA round 5, L-1). It read:
///
/// ```ignore
/// const STAMP_CAP: usize = ISO8601_LEN + 8;
/// const _: () = assert!(STAMP_CAP >= ISO8601_LEN, "…smaller than the timestamp…");
/// ```
///
/// which is `X + 8 >= X`. It cannot fail for any definition of `ISO8601_LEN`, and the
/// prose above it claimed it "fails the BUILD if the format ever outgrows the buffer".
/// That is the identical shape as the comment it replaced — a claim of a coupling that
/// nothing implements — and it was *more* convincing, because this time there was code
/// under it. A check that cannot fail is worse than no check: it advertises coverage
/// and denies it at the same time.
///
/// The property is genuinely not expressible at compile time. `write_iso8601` renders
/// through `{:04}`, whose width is a MINIMUM, so the longest string it can produce is a
/// function of the runtime clock rather than of any constant. So the guard is a runtime
/// test over the inputs that discriminate — an ordinary year, year 10000, and further
/// still — asserting the buffer's own `truncated()` flag and the terminator:
/// [`tests::an_absurd_clock_does_not_silently_clip_the_crash_timestamp`]. That test can
/// fail, has a positive control, and is the only thing here that holds this property.
/// Naming where the guard actually lives is the point; a vacuous `assert!` alongside it
/// would only re-create the impression that the constant is self-checking.
const STAMP_CAP: usize = super::timefmt::ISO8601_LEN + 8;

/// Open the pre-computed report path, falling back to stderr so a crash is never
/// *completely* silent (a Plasma-launched run's stderr reaches journald).
fn open_report() -> c_int {
    let path = REPORT_PATH.load(Ordering::Acquire);
    if path.is_null() {
        return libc::STDERR_FILENO;
    }
    // `O_APPEND`, never `O_TRUNC` (QA round 5, H-2). `SIGABRT` is in `FATAL_SIGNALS`, and
    // the ordinary way this process reaches it is a panic in a gtk-rs `extern "C"`
    // callback: the panic hook writes a full report naming the panic and its location,
    // the runtime then aborts because it cannot unwind across the C frame, and this
    // handler runs on the resulting `SIGABRT` — over the very report that explains it.
    // Truncating here threw away the most informative artefact in favour of the least
    // (`signal: SIGABRT` tells a reader nothing a panic message would not have).
    //
    // The path carries the run's start time and pid, so appending can only ever
    // accumulate faults from THIS dying run. `O_APPEND` is also the async-signal-safe
    // choice: the kernel makes the seek-and-write atomic, so this handler needs no
    // position to track and cannot interleave with the panic hook's `File`.
    //
    // SAFETY: `path` is a leaked NUL-terminated CString from `install`.
    let fd = unsafe {
        libc::open(
            path,
            libc::O_WRONLY | libc::O_CREAT | libc::O_APPEND,
            // The seam's CONSTANT, shared with `private_options` (L-7). This call cannot
            // use the constructor — `OpenOptions::open` allocates and is not
            // async-signal-safe — but it can share the value, which is the half that
            // could drift.
            super::PRIVATE_FILE_MODE,
        )
    };
    if fd < 0 {
        libc::STDERR_FILENO
    } else {
        fd
    }
}

/// `write(2)`, retrying short writes and `EINTR`. The only output primitive here.
fn write_bytes(fd: c_int, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        // SAFETY: writing `bytes.len()` bytes from a live slice to an open fd.
        let written = unsafe { libc::write(fd, bytes.as_ptr().cast::<c_void>(), bytes.len()) };
        if written > 0 {
            bytes = &bytes[written as usize..];
            continue;
        }
        let interrupted = written < 0 && errno() == libc::EINTR;
        if !interrupted {
            return; // The descriptor is unusable; the report ends here.
        }
    }
}

/// The calling thread's `errno`, through the platform's async-signal-safe
/// accessor. The accessor itself is not portable — glibc names it
/// `__errno_location`, the BSD family (macOS included) `__error` — though what
/// it returns is the same: a live, per-thread pointer, readable with no
/// allocation and no locking, which is why either is legal to call from the
/// handler. An unrecognised target reports a value that can never equal
/// `EINTR`, so a write is simply not retried there — degrade like
/// `instruction_pointer`'s "unavailable" branch above, never guess.
#[cfg(target_os = "linux")]
fn errno() -> c_int {
    // SAFETY: `__errno_location` returns a valid, non-null pointer to the
    // calling thread's own errno cell.
    unsafe { *libc::__errno_location() }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn errno() -> c_int {
    // SAFETY: as above, via the BSD-family accessor.
    unsafe { *libc::__error() }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "ios")))]
fn errno() -> c_int {
    0
}

/// Wall clock inside a signal handler. `clock_gettime` is async-signal-safe;
/// `SystemTime::now` is not documented to be.
fn realtime_unix_millis() -> Option<i64> {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is a live, correctly-typed out-parameter.
    if unsafe { libc::clock_gettime(libc::CLOCK_REALTIME, &mut ts) } != 0 {
        return None;
    }
    // No casts: `time_t`/`tv_nsec` are already `i64` on every 64-bit target this
    // application supports, and clippy rejects both a redundant `as` and a
    // redundant `From`. A 32-bit unix port would need widening here.
    Some(ts.tv_sec * 1_000 + ts.tv_nsec / 1_000_000)
}

/// The fault block: which signal, at which address, from which instruction.
///
/// The instruction pointer is what makes a report comparable with the kernel's own
/// `segfault at … ip …` line, and — with the module map written below — what the
/// plan's evidence-recovery arithmetic consumes (TDD 21.8).
fn write_fault(fd: c_int, signal: c_int, info: *mut libc::siginfo_t, context: *mut c_void) {
    let mut buf: RawBuf<160> = RawBuf::new();
    use core::fmt::Write as _;
    let _ = writeln!(buf, "\nsignal: {} ({signal})", signal_name(signal));

    if info.is_null() {
        let _ = writeln!(buf, "fault address: (unavailable)");
    } else {
        // SAFETY: the kernel supplies a valid `siginfo_t` for an SA_SIGINFO handler.
        let address = unsafe { (*info).si_addr() } as usize;
        let _ = writeln!(buf, "fault address: {address:#x}");
    }

    match instruction_pointer(context) {
        Some(ip) => {
            let _ = writeln!(buf, "instruction pointer: {ip:#x}");
        }
        None => {
            let _ = writeln!(buf, "instruction pointer: (unavailable on this target)");
        }
    }
    write_bytes(fd, buf.as_bytes());
}

fn signal_name(signal: c_int) -> &'static str {
    match signal {
        libc::SIGSEGV => "SIGSEGV",
        libc::SIGBUS => "SIGBUS",
        libc::SIGILL => "SIGILL",
        libc::SIGABRT => "SIGABRT",
        libc::SIGTRAP => "SIGTRAP",
        _ => "unknown",
    }
}

/// The faulting instruction pointer, read from the machine context.
///
/// Architecture-specific by nature — the register lives in a different place on
/// each. Unsupported targets report "unavailable" rather than guessing; the module
/// map and breadcrumbs still land.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn instruction_pointer(context: *mut c_void) -> Option<usize> {
    if context.is_null() {
        return None;
    }
    // SAFETY: for an SA_SIGINFO handler the third argument is a `ucontext_t*`.
    unsafe {
        let context = &*context.cast::<libc::ucontext_t>();
        Some(context.uc_mcontext.gregs[libc::REG_RIP as usize] as usize)
    }
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
fn instruction_pointer(context: *mut c_void) -> Option<usize> {
    if context.is_null() {
        return None;
    }
    // SAFETY: as above.
    unsafe { Some((*context.cast::<libc::ucontext_t>()).uc_mcontext.pc as usize) }
}

#[cfg(not(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
fn instruction_pointer(_context: *mut c_void) -> Option<usize> {
    None
}

/// The breadcrumbs — what the application was doing (TDD 21.5).
fn write_breadcrumbs(fd: c_int) {
    write_bytes(fd, b"\n--- breadcrumbs, oldest first ---\n");
    let ring = BREADCRUMBS.load(Ordering::Acquire);
    if ring.is_null() {
        write_bytes(fd, b"(unavailable)\n");
        return;
    }
    // SAFETY: a `&'static Ring` published at install time and never freed.
    let ring = unsafe { &*ring };
    let mut any = false;
    ring.for_each(|line| {
        any = true;
        write_bytes(fd, line);
        write_bytes(fd, b"\n");
    });
    if !any {
        write_bytes(fd, b"(none recorded)\n");
    }
}

/// Frames, last because they are the least useful half here: the shipped binary is
/// stripped and the distribution's GTK carries no symbols (ScrAP-141), so these
/// resolve as `module(+offset)` — which is precisely what the module map below is
/// for.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn write_backtrace(fd: c_int) {
    const MAX_FRAMES: usize = 64;
    write_bytes(fd, b"\n--- backtrace ---\n");
    let mut frames: [*mut c_void; MAX_FRAMES] = [std::ptr::null_mut(); MAX_FRAMES];
    // SAFETY: `backtrace` fills at most `MAX_FRAMES` entries of a live array, and
    // `backtrace_symbols_fd` writes to the descriptor without allocating (unlike
    // `backtrace_symbols`, which mallocs and must never be used here).
    unsafe {
        let count = libc::backtrace(frames.as_mut_ptr(), MAX_FRAMES as c_int);
        libc::backtrace_symbols_fd(frames.as_ptr(), count, fd);
    }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn write_backtrace(fd: c_int) {
    write_bytes(fd, b"\n--- backtrace ---\n(unavailable on this platform)\n");
}

/// The executable mappings of the process, so every recorded address resolves to
/// *module + offset* by subtraction alone (TDD 21.8).
///
/// This is the table the plan's evidence-recovery method had to reconstruct by hand
/// from `readelf` after the fact; recording it at fault time makes a report
/// self-contained. Only `x`-permitted mappings are kept — the address of a faulting
/// instruction is always in one, and the rest would triple the report.
#[cfg(target_os = "linux")]
fn write_module_map(fd: c_int) {
    write_bytes(fd, b"\n--- executable mappings ---\n");
    // SAFETY: a NUL-terminated literal path, opened read-only.
    let maps = unsafe { libc::open(c"/proc/self/maps".as_ptr(), libc::O_RDONLY) };
    if maps < 0 {
        write_bytes(fd, b"(unavailable)\n");
        return;
    }

    // Reassembled byte-wise rather than through `RawBuf`: a mapping line is raw
    // bytes (a path need not be UTF-8 on unix), and `RawBuf` exists to keep its
    // contents a valid `str`. Nothing here formats, so nothing here needs it.
    const MAX_LINE: usize = 512;
    let mut chunk = [0u8; 4096];
    let mut line = [0u8; MAX_LINE];
    let mut line_len = 0usize;
    loop {
        // SAFETY: reading at most `chunk.len()` bytes into a live buffer.
        let read = unsafe { libc::read(maps, chunk.as_mut_ptr().cast::<c_void>(), chunk.len()) };
        if read <= 0 {
            break;
        }
        for &byte in &chunk[..read as usize] {
            if byte == b'\n' {
                if is_executable_mapping(&line[..line_len]) {
                    write_bytes(fd, &line[..line_len]);
                    write_bytes(fd, b"\n");
                }
                line_len = 0;
            } else if line_len < MAX_LINE {
                // A path longer than the buffer truncates; the load address, which
                // is what the arithmetic needs, is at the start of the line.
                line[line_len] = byte;
                line_len += 1;
            }
        }
    }
    // SAFETY: closing a descriptor this function opened.
    unsafe { libc::close(maps) };
}

/// Whether a `/proc/self/maps` line describes an executable mapping.
///
/// The line is `<start>-<end> <perms> …`, so the permission field starts three
/// bytes after the first space and its third character is `x` when executable.
///
/// Not `#[cfg(target_os = "linux")]` despite `/proc` being Linux-only: the
/// parsing itself is plain byte matching with no OS dependency, so it stays
/// compiled (and unit-tested) on every unix this module supports. Only
/// `write_module_map`'s caller of it — which actually opens `/proc/self/maps` —
/// is Linux-gated, which is why a non-Linux, non-test build has no caller left.
#[cfg_attr(not(any(test, target_os = "linux")), allow(dead_code))]
fn is_executable_mapping(line: &[u8]) -> bool {
    let Some(space) = line.iter().position(|b| *b == b' ') else {
        return false;
    };
    line.get(space + 3) == Some(&b'x')
}

#[cfg(not(target_os = "linux"))]
fn write_module_map(fd: c_int) {
    write_bytes(
        fd,
        b"\n--- executable mappings ---\n(unavailable on this platform)\n",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Everything in `whole` before the test module — the half that can run inside a
    /// signal handler.
    ///
    /// **Why this is not a substring search, at some length, because the obvious thing
    /// is what shipped and it was silently wrong for a whole review round.**
    ///
    /// The check above was `whole.split("#[cfg(test)]").next().unwrap_or(whole)`. That
    /// takes the FIRST occurrence of the literal anywhere in the file — and this file's
    /// own prose contains it: the `STAMP_CAP` doc comment says *"its `truncated` flag is
    /// `#[cfg(test)]`"*. So the scan read the first ~281 lines of a 797-line file and was
    /// structurally blind to lines 282–555, which is where `open_report`, `write_fault`,
    /// `write_breadcrumbs`, `write_backtrace` and `write_module_map` live — every function
    /// that actually writes the report from inside the handler. It passed only because the
    /// four allowed names happen to sit above the accidental boundary. Injecting
    /// `super::sink::flush()` into `write_breadcrumbs` left it GREEN (QA round 5, M-1).
    ///
    /// That is the second time prose has poisoned this check, and the first fix was
    /// rebuilt from the instance that broke rather than from the population that could
    /// (ScrAP-220): the boundary was chosen so that *the prose known at the time* fell on
    /// the right side of it. New prose moved it again. So the rule here ranges over the
    /// population instead:
    ///
    /// 1. **A whole line, trimmed** — prose mentions the attribute inside a `///` comment
    ///    or a string, never as a line that consists of nothing else. This alone fixes the
    ///    observed bug.
    /// 2. **Exactly one such line** — not "the first". "First" and "last" are both
    ///    guesses about which match is the real one, and a guess that silently picks
    ///    wrong is precisely the failure being repaired. If a second `#[cfg(test)]` item
    ///    ever appears at top level, this fails and asks rather than choosing.
    /// 3. **Followed by `mod tests {`** — a `#[cfg(test)]` on a *function or field* in the
    ///    implementation half would satisfy (1) and (2) and silently truncate the scan all
    ///    over again. Requiring the module declaration pins the boundary to the thing it
    ///    is meant to be.
    /// 4. **No silent fallback.** The old `unwrap_or(whole)` degraded quietly to scanning
    ///    everything, which fails in the *other* direction (the test module's own prose
    ///    names the forbidden modules) and would read as a mysterious unrelated failure.
    ///    Every way of not finding the boundary now says so in its own words.
    ///
    /// The general lesson, which is worth more than this function: a delimiter derived
    /// from source text must be identified by something the *language* guarantees is
    /// unique, not by a string that also occurs in the prose describing it.
    fn implementation_half(whole: &str) -> &str {
        const ATTR: &str = "#[cfg(test)]";
        const MODULE: &str = "mod tests {";

        let mut boundaries = Vec::new();
        let mut offset = 0usize;
        // `split_inclusive` keeps the newline, so `offset` stays byte-exact regardless of
        // line endings; `trim` then removes the `\n` (and any `\r`) along with indentation.
        for line in whole.split_inclusive('\n') {
            if line.trim() == ATTR {
                boundaries.push(offset);
            }
            offset += line.len();
        }

        assert_eq!(
            boundaries.len(),
            1,
            "expected exactly one line in signal.rs that is nothing but `{ATTR}` — the \
             attribute on the test module — but found {}. This boundary decides which \
             half of the file the async-signal-safety scan can see, and picking one of \
             several candidates is the exact silent-wrong-answer this check was rebuilt \
             to stop doing. Say which one is the boundary rather than letting it guess.",
            boundaries.len()
        );
        let boundary = boundaries[0];

        let after = whole[boundary..]
            .split_inclusive('\n')
            .nth(1)
            .unwrap_or_default()
            .trim();
        assert_eq!(
            after, MODULE,
            "the line after signal.rs's `{ATTR}` is `{after}`, not `{MODULE}`. A \
             `{ATTR}` on anything other than the test module would truncate the \
             async-signal-safety scan silently — which is how it came to audit 281 of \
             797 lines and report success."
        );

        &whole[..boundary]
    }

    /// The boundary finder finds the boundary — and the region it returns is the one
    /// the safety scan needs, not a prefix of it.
    ///
    /// A REACH control, not a completeness gate (that distinction is ScrAP-216's): it
    /// does not claim to enumerate what the handler calls, only that the scanned region
    /// extends past the prose that truncated it and over the functions that write the
    /// report. Those names are stable and load-bearing; if one is renamed this fails and
    /// the next author re-confirms the reach deliberately, which is the outcome wanted.
    #[test]
    fn the_safety_scan_can_see_the_whole_implementation() {
        let whole = include_str!("signal.rs");
        let source = implementation_half(whole);

        // The prose that hijacked the old boundary is INSIDE the scanned region — i.e.
        // the scan no longer stops at it.
        assert!(
            source.contains("its `truncated` flag is"),
            "the boundary is still landing on the STAMP_CAP doc comment"
        );
        // …and the region reaches the report writers, which is what M-1 could not see.
        for reached in [
            "fn open_report",
            "fn write_fault",
            "fn write_breadcrumbs",
            "fn write_backtrace",
            "fn write_module_map",
        ] {
            assert!(
                source.contains(reached),
                "the async-signal-safety scan cannot see `{reached}` — it is green \
                 because it is blind, which is the defect, not the check"
            );
        }
        // The other direction: the test module's own prose (which names the forbidden
        // modules in plain text) must stay OUT, or the scan fails for the wrong reason.
        assert!(
            !source.contains("mod tests {"),
            "the scanned region has swallowed the test module"
        );
    }

    /// The crash timestamp survives an absurd clock without silent truncation.
    ///
    /// `{:04}` on the year is a MINIMUM width, so a machine reading year 10000 renders
    /// 25 bytes against an `ISO8601_LEN` of 24. `RawBuf` truncates silently in release,
    /// so sizing the buffer to exactly `ISO8601_LEN` — the obvious way to make
    /// `timefmt`'s prose true — would have made the report lose the tail of its
    /// timestamp precisely when the clock is wrong, which is disproportionately when a
    /// machine is misbehaving.
    ///
    /// This asserts the property, not the number: `truncated()` is the buffer's own
    /// report, so it stays honest if either the format or `STAMP_CAP` changes.
    #[test]
    fn an_absurd_clock_does_not_silently_clip_the_crash_timestamp() {
        for millis in [
            1_785_290_079_123_i64, // an ordinary 2026 instant
            253_402_300_800_000,   // year 10000 — five digits
            2_534_023_008_000_000, // further still
        ] {
            let mut stamp: RawBuf<STAMP_CAP> = RawBuf::new();
            let _ = super::super::timefmt::civil_from_unix_millis(millis).write_iso8601(&mut stamp);
            assert!(
                !stamp.truncated(),
                "the crash timestamp was clipped at {millis} — a report that loses its \
                 timestamp when the clock is absurd is a report that loses it exactly \
                 when it matters"
            );
            // The tail is what a too-small buffer eats, so assert the tail directly:
            // a clipped stamp loses its `Z` before it loses anything a reader misses.
            assert!(
                stamp.as_str().ends_with('Z'),
                "timestamp {:?} lost its terminator at {millis}",
                stamp.as_str()
            );
        }
    }

    /// **The reachability matrix, as a check rather than a paragraph.**
    ///
    /// Everything this module reaches from the handler inherits async-signal-safety:
    /// no allocation, no locks, no reentrant libc. Established empirically from the
    /// call graph in QA round 4:
    ///
    /// | reachable from the handler | NOT reachable                      |
    /// |----------------------------|------------------------------------|
    /// | `rawbuf`, `timefmt`, `ring`| `sink`, `report`, `identity`, `logging` |
    ///
    /// A module moving from the right column to the left inherits the whole
    /// constraint **silently** — nothing in the type system says so, and the compiler
    /// is perfectly happy to let a handler call `malloc` through three layers of
    /// someone else's helper. That is a constraint held only by prose, which is one
    /// refactor from being violated; this test is what makes the prose fail.
    ///
    /// It reads THIS FILE's own source, so it cannot drift from what the file
    /// actually says. Deliberately an ALLOWLIST of the exact reference set rather
    /// than a denylist of forbidden modules: a denylist protects only the names
    /// someone thought of, while this fails on anything new and makes adding it a
    /// decision with a justification attached (ScrAP-220 — range over the population,
    /// not over the instances you remembered).
    ///
    /// Widening it is legitimate. Widening it without arguing async-signal-safety for
    /// the new module is the bug.
    ///
    /// **This check has now been poisoned by prose twice**, and [`implementation_half`]
    /// is the answer to the second time. See it for why the boundary is a *unique,
    /// structurally-verified* line rather than a substring search.
    #[test]
    fn the_handler_reaches_only_async_signal_safe_modules() {
        // Only the IMPLEMENTATION half. The test module below never runs inside a
        // signal handler, and its own prose mentions the forbidden module names — a
        // first version scanned the whole file and failed on the text of this very
        // assertion, which is a neat demonstration of why the boundary matters.
        let whole = include_str!("signal.rs");
        let source = implementation_half(whole);
        let mut found: Vec<&str> = source
            .match_indices("super::")
            .map(|(i, _)| {
                let rest = &source[i + "super::".len()..];
                let end = rest
                    .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .unwrap_or(rest.len());
                &rest[..end]
            })
            .filter(|ident: &&str| !ident.is_empty())
            .collect();
        found.sort_unstable();
        found.dedup();

        // `install` is not a module — it is mod.rs's installer, named in a doc link.
        // Pinned anyway, so the set is exact and any change to it is deliberate.
        // (`Ring` the TYPE arrives via `use super::ring::Ring`, so it is covered by
        // `ring`; it was in this list until the test measured otherwise.)
        // `install` is not a module — see above. `PRIVATE_FILE_MODE` is not one either:
        // it is a `const u32` in `super`, so the reference compiles to an immediate and
        // reaches no code, no allocator and no lock. It is the mode `open_report` passes
        // to `libc::open`, shared with `private_options` so the seam's constant cannot
        // drift from its constructor (L-7) — the value crosses the boundary, the call
        // does not.
        //
        // Worth recording HOW this arrived: adding that reference made this assertion
        // FAIL, which is the first time it has ever fired. It could not have: the
        // reference sits at the `libc::open` call, inside the 282-555 band the old
        // `split("#[cfg(test)]")` boundary could not see (M-1). The gate started working
        // and immediately caught the next widening — including one of my own.
        let allowed = ["PRIVATE_FILE_MODE", "install", "rawbuf", "ring", "timefmt"];
        assert_eq!(
            found, allowed,
            "the set of `super::` references from the fatal-signal handler changed.\n\
             Everything reachable here must be async-signal-safe: no allocation, no \
             locks, no reentrant libc. `sink`, `report`, `identity` and `logging` are \
             NOT — they allocate and take locks.\n\
             If the new reference is genuinely safe, add it to `allowed` together with \
             the argument for why. Do not widen this silently."
        );
    }

    #[test]
    fn every_reported_signal_has_a_name() {
        for signal in FATAL_SIGNALS {
            assert_ne!(signal_name(signal), "unknown", "unnamed signal {signal}");
        }
        assert_eq!(signal_name(libc::SIGTERM), "unknown");
    }

    #[test]
    fn executable_mappings_are_told_from_the_rest() {
        // Real lines from a running instance: the code segment is kept, the data
        // and heap segments are not.
        assert!(is_executable_mapping(
            b"7f2c1a000000-7f2c1a09b000 r-xp 00025000 08:02 1 /usr/lib/libgtk-4.so.1.600.9"
        ));
        assert!(!is_executable_mapping(
            b"7f2c1a09b000-7f2c1a0a0000 r--p 000c0000 08:02 1 /usr/lib/libgtk-4.so.1.600.9"
        ));
        assert!(!is_executable_mapping(b"5602d1c00000-5602d1c21000 rw-p"));
        assert!(!is_executable_mapping(b""));
        assert!(!is_executable_mapping(b"garbage-with-no-space"));
    }

    /// The whole handler, driven by a real signal, in a real dying process.
    ///
    /// Serialises the tests that call [`install`].
    ///
    /// `install` publishes into process-wide statics (`REPORT_PATH`, `HEADER_PTR`,
    /// `BREADCRUMBS`). Two such tests running concurrently — libtest's default — would
    /// have one overwrite the other's report path and then assert against a file the
    /// other test's child wrote to, or to none at all. The failure would be
    /// intermittent and would read as a defect in the handler.
    ///
    /// Not `cfg`-gated to Linux, even though both its users skip at runtime on every
    /// other platform: the tests that take it are compiled everywhere now, so a gate
    /// here would make the merge of those two changes fail to build on macOS. The lock
    /// costs nothing where the bodies return early.
    static INSTALL_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// The lock above, plus everything [`install`] displaces in the process — put back
    /// when the test that armed the handler ends.
    ///
    /// **Why this exists.** [`install`] deliberately has no uninstall: production arms
    /// the handler once and wants it for the whole process lifetime. A *test* process
    /// is the one place where that is wrong, because libtest runs the entire suite in
    /// one process — so a handler armed by a test here stayed armed over every test
    /// after it, with that test's leaked `Ring` still published. Two consequences, both
    /// measured:
    ///
    /// * any later fatal signal in the suite wrote a **complete, plausible
    ///   Scribobulate crash report** whose breadcrumbs were this module's fixture
    ///   (`opened /tmp/a.md`, `monitor event`). A reader's first conclusion was that
    ///   the application had crashed — the report named the wrong process, the wrong
    ///   activity, and a defect that did not exist;
    /// * arming necessarily displaces Rust's own stack-overflow handler for SIGSEGV,
    ///   and `install_alternate_stack` replaces the calling thread's alternate stack
    ///   with it (this module's own header says so). A stack overflow later in the
    ///   suite therefore stopped being a per-thread "has overflowed its stack" abort
    ///   and became a process-wide SIGSEGV — measured both ways with the same probe,
    ///   which is what turned the second bullet from a hypothesis into a cause.
    ///
    /// ScrAP-265 holds the whole account, including the half that generalises past
    /// signals.
    ///
    /// **Restoring is test-only on purpose.** Exporting an `uninstall` from the module
    /// would add a `pub(crate)` item with no production caller — the same argument
    /// [`install_alternate_stack`] already records for staying private — and a
    /// production uninstall is a *capability nobody wants*: a window in which the
    /// process can die unreported.
    struct ArmedHandler {
        _lock: std::sync::MutexGuard<'static, ()>,
        previous: [libc::sigaction; FATAL_SIGNALS.len()],
        previous_stack: libc::stack_t,
        restored: bool,
    }

    impl ArmedHandler {
        /// Take the install lock, record what this process looks like *now*, and arm
        /// the handler.
        ///
        /// The snapshot is read back out of the kernel rather than assumed: what
        /// SIGSEGV points at before this call is Rust's stack-overflow handler, which
        /// is nothing this module could reconstruct.
        fn arm(report_path: &std::path::Path, header: &str, ring: &'static Ring) -> Self {
            let lock = INSTALL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            // SAFETY: both out-parameters are live and correctly typed; a null `act`
            // queries the current disposition without changing it, and a null first
            // argument to `sigaltstack` likewise queries the calling thread's stack.
            let (previous, previous_stack) = unsafe {
                let mut previous: [libc::sigaction; FATAL_SIGNALS.len()] = std::mem::zeroed();
                for (slot, signal) in previous.iter_mut().zip(FATAL_SIGNALS) {
                    libc::sigaction(signal, std::ptr::null(), slot);
                }
                let mut previous_stack: libc::stack_t = std::mem::zeroed();
                libc::sigaltstack(std::ptr::null(), &mut previous_stack);
                (previous, previous_stack)
            };

            install(report_path, header, ring);

            Self {
                _lock: lock,
                previous,
                previous_stack,
                restored: false,
            }
        }

        /// Put the process back where [`arm`](Self::arm) found it.
        ///
        /// Idempotent, so a test may call it explicitly — to observe the disarmed
        /// process while still holding the lock — and `Drop` can then call it
        /// unconditionally.
        ///
        /// The 64 KiB alternate stack `install` leaked stays leaked. It is unreachable
        /// once the previous stack is back, and freeing memory a signal handler might
        /// still be standing on is not a trade worth making to tidy a test process.
        fn disarm(&mut self) {
            if self.restored {
                return;
            }
            self.restored = true;
            // SAFETY: every value restored here was read out of this same process, on
            // this same thread, by `arm`.
            unsafe {
                for (slot, signal) in self.previous.iter().zip(FATAL_SIGNALS) {
                    libc::sigaction(signal, slot, std::ptr::null_mut());
                }
                libc::sigaltstack(&self.previous_stack, std::ptr::null_mut());
            }
        }

        /// What SIGSEGV pointed at before this guard armed the handler.
        ///
        /// Read from the guard rather than by the caller, because the caller cannot
        /// take the snapshot safely: the install lock is what makes "what does SIGSEGV
        /// point at" a stable question, and it is held from [`arm`](Self::arm) onwards.
        /// A test that read it a line earlier could sample a *sibling* test's armed
        /// handler and then assert that the process was restored to it — measured, not
        /// theorised; that is how the first draft of the guard test failed.
        fn displaced_sigsegv_handler(&self) -> libc::sighandler_t {
            let index = FATAL_SIGNALS
                .iter()
                .position(|signal| *signal == libc::SIGSEGV)
                .expect("SIGSEGV is one of the signals this handler takes");
            self.previous[index].sa_sigaction
        }

        /// The alternate signal stack this thread had before arming, as the triple
        /// [`current_alternate_stack`] reports.
        fn displaced_alternate_stack(&self) -> (*mut c_void, usize, c_int) {
            let stack = self.previous_stack;
            (stack.ss_sp, stack.ss_size, stack.ss_flags)
        }
    }

    impl Drop for ArmedHandler {
        fn drop(&mut self) {
            self.disarm();
        }
    }

    /// The disposition SIGSEGV currently carries, as the raw handler address.
    fn current_sigsegv_handler() -> libc::sighandler_t {
        // SAFETY: a live out-parameter and a null `act`, which queries only.
        unsafe {
            let mut current: libc::sigaction = std::mem::zeroed();
            libc::sigaction(libc::SIGSEGV, std::ptr::null(), &mut current);
            current.sa_sigaction
        }
    }

    /// The calling thread's alternate signal stack, as the triple that identifies it.
    fn current_alternate_stack() -> (*mut c_void, usize, c_int) {
        // SAFETY: a live out-parameter and a null first argument, which queries only.
        unsafe {
            let mut current: libc::stack_t = std::mem::zeroed();
            libc::sigaltstack(std::ptr::null(), &mut current);
            (current.ss_sp, current.ss_size, current.ss_flags)
        }
    }

    /// Fork a child that does nothing but die by `signal`, wait for it, and return the
    /// wait status so a caller can assert *how* it died.
    ///
    /// The child drops its core limit first, so a `core_pattern` handler is not
    /// invoked for a deliberate crash; `_exit` guards against the raise returning,
    /// which it does whenever the disposition in force is one that resumes.
    ///
    /// `install` runs in the **parent** in every caller, before the fork, deliberately:
    /// it allocates, and allocating in a forked child of a multi-threaded process can
    /// deadlock on a `malloc` lock another thread held at fork time. The child inherits
    /// the handlers and the alternate stack and does nothing but raise.
    fn a_child_raises(signal: c_int) -> c_int {
        // SAFETY: the child touches nothing that allocates or locks.
        let child = unsafe { libc::fork() };
        assert!(child >= 0, "fork failed");
        if child == 0 {
            unsafe {
                let no_core = libc::rlimit {
                    rlim_cur: 0,
                    rlim_max: 0,
                };
                libc::setrlimit(libc::RLIMIT_CORE, &no_core);
                libc::raise(signal);
                libc::_exit(0);
            }
        }
        let mut status = 0;
        // SAFETY: reaping the child forked above.
        unsafe { libc::waitpid(child, &mut status, 0) };
        status
    }

    /// A forked child is the only honest harness here: the thing under test *kills
    /// the process*, so it cannot run in the test process, and mocking the signal
    /// away would leave every interesting part — the pre-computed path, the
    /// re-raise, the ordering under a real fault — unexercised. It covers TDD 21.4
    /// (names the fault, still dies from it), 21.5 (breadcrumbs), 21.6 (order) and
    /// 21.8 (module map) at once, because they are properties of one artefact.
    ///
    /// `install` runs in the **parent**, before the fork, deliberately: it allocates,
    /// and allocating in a forked child of a multi-threaded process can deadlock on
    /// a `malloc` lock another thread held at fork time. The child inherits the
    /// handlers and the alternate stack and does nothing but raise.
    #[test]
    fn a_fatal_signal_leaves_a_full_report_and_still_kills_the_process() {
        if !cfg!(all(target_os = "linux", target_env = "gnu")) {
            eprintln!(
                "SKIPPED [TDD 21.4/21.5/21.6/21.8 fatal-signal report]: needs glibc \
                 backtrace and /proc/self/maps, neither available on this platform. \
                 NOT verified by this run."
            );
            return;
        }
        // Armed AFTER the skip: a platform that cannot run the body has no `install`
        // to serialise against, and holding a lock across an early return is noise.
        // The guard also DISARMS on the way out — see [`ArmedHandler`] for what an
        // un-disarmed handler cost the rest of the suite.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash-test.log");
        let ring: &'static Ring = Box::leak(Box::new(Ring::new()));
        ring.record("2026-07-29T01:54:00.000Z INFO  scribobulate::app::open: opened /tmp/a.md");
        ring.record("2026-07-29T01:54:38.000Z INFO  scribobulate::window::reload: monitor event");

        let _armed = ArmedHandler::arm(
            &path,
            "scribobulate 0.1.0 (test, release)\npid: test\n",
            ring,
        );

        let status = a_child_raises(libc::SIGSEGV);

        // TDD 21.4: the process still dies from the signal it was killed by.
        assert!(
            libc::WIFSIGNALED(status),
            "child exited normally (status {status}) — the handler swallowed the signal"
        );
        assert_eq!(libc::WTERMSIG(status), libc::SIGSEGV);

        let text = std::fs::read_to_string(&path).expect("no crash report was written");
        for expected in [
            "=== scribobulate crash report ===",
            "kind: signal",
            "scribobulate 0.1.0 (test, release)", // the identity header
            "signal: SIGSEGV",
            "fault address: 0x",
            "instruction pointer: 0x",
            "opened /tmp/a.md", // TDD 21.5
            "monitor event",
            "--- backtrace ---",
            "--- executable mappings ---",
        ] {
            assert!(text.contains(expected), "missing {expected:?} in:\n{text}");
        }

        // TDD 21.6: identity and fault before breadcrumbs before the backtrace.
        let identity = text.find("scribobulate 0.1.0").unwrap();
        let fault = text.find("signal: SIGSEGV").unwrap();
        let crumbs = text.find("opened /tmp/a.md").unwrap();
        let backtrace = text.find("--- backtrace ---").unwrap();
        assert!(
            identity < fault && fault < crumbs && crumbs < backtrace,
            "{text}"
        );

        // TDD 21.8: a mapping line carries the module and its load address, so a
        // recorded instruction pointer resolves by subtraction alone.
        let mapping = text
            .lines()
            .skip_while(|l| !l.starts_with("--- executable mappings ---"))
            .find(|l| l.contains(".so") || l.contains("scribobulate"))
            .expect("no named executable mapping in the report");
        assert!(
            mapping.starts_with(|c: char| c.is_ascii_hexdigit()) && mapping.contains('-'),
            "mapping line has no load address: {mapping:?}"
        );
    }

    /// **The enumeration is asserted, not trusted** — every signal in
    /// [`FATAL_SIGNALS`] is actually taken, reported by name, and re-raised.
    ///
    /// The sibling above proves the whole artefact for one signal. This proves the
    /// *set*, which is the half that was wrong: the list said four, and a GLib fatal
    /// message dies of a fifth (`SIGTRAP`), so that entire class of death — `g_error`,
    /// and every warning `G_DEBUG=fatal-warnings` promotes — left no report at all
    /// (ScrAP-268). A list nothing ranges over is a list that silently stops matching
    /// what the process can die of, so adding a signal here now costs a line and
    /// removing one fails loudly.
    ///
    /// Cheap to widen, and deliberately assertion-per-signal rather than "some report
    /// appeared": `signal: <name>` is what makes a report say *which* death it
    /// describes, and `signal_name` returning `"unknown"` would still write a file.
    ///
    /// **The expected names are written out here rather than read from
    /// [`signal_name`]** — deriving them from the code under test makes the test agree
    /// with itself, which is exactly what happened in the round that wrote it: unnaming
    /// `SIGTRAP` left this green while the report said `signal: unknown (5)`
    /// (GTK4Rs/AP-160's shape, caught by mutation rather than by reading).
    ///
    /// **Redundancy, recorded honestly** (ScrAP-254): the length check below *also*
    /// fails if `SIGTRAP` is dropped from [`FATAL_SIGNALS`], which
    /// [`a_glib_fatal_message_dies_by_a_signal_this_handler_takes`] catches too. They
    /// are not the same claim — this one holds that the const was not quietly narrowed,
    /// that one holds that GLib still dies there — and neither implies the other.
    #[test]
    fn every_enumerated_fatal_signal_is_reported_and_still_kills_the_process() {
        if !cfg!(all(target_os = "linux", target_env = "gnu")) {
            eprintln!(
                "SKIPPED [TDD 21.4 fatal-signal enumeration]: needs a forked child \
                 raising real signals against a glibc handler. NOT verified by this run."
            );
            return;
        }
        const EXPECTED: [(c_int, &str); 5] = [
            (libc::SIGSEGV, "SIGSEGV"),
            (libc::SIGBUS, "SIGBUS"),
            (libc::SIGILL, "SIGILL"),
            (libc::SIGABRT, "SIGABRT"),
            (libc::SIGTRAP, "SIGTRAP"),
        ];
        assert_eq!(
            FATAL_SIGNALS.len(),
            EXPECTED.len(),
            "FATAL_SIGNALS changed without this table: every signal the handler takes \
             is driven through a real death here, so name the new one (or say why the \
             removed one can no longer kill this process)"
        );

        for (signal, name) in EXPECTED {
            assert!(
                FATAL_SIGNALS.contains(&signal),
                "{name} is not in FATAL_SIGNALS"
            );
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("crash-enumerated.log");
            let ring: &'static Ring = Box::leak(Box::new(Ring::new()));
            ring.record("2026-08-11T10:00:00.000Z INFO  scribobulate::app::open: opened /tmp/a.md");

            let _armed = ArmedHandler::arm(
                &path,
                "scribobulate 0.1.0 (test, release)\npid: test\n",
                ring,
            );
            let status = a_child_raises(signal);

            assert!(
                libc::WIFSIGNALED(status),
                "a child that raised {name} exited normally (status {status}) — the \
                 handler swallowed it"
            );
            assert_eq!(
                libc::WTERMSIG(status),
                signal,
                "{name} was re-raised as a different signal"
            );

            let text = std::fs::read_to_string(&path).unwrap_or_else(|_| {
                panic!("{name} is in FATAL_SIGNALS but its death left no crash report")
            });
            assert!(
                text.contains(&format!("signal: {name} ({signal})")),
                "the report for {name} does not name it:\n{text}"
            );
            assert!(text.contains("opened /tmp/a.md"), "{name}: no breadcrumbs");
        }
    }

    /// The report path a doomed child writes to, handed over the environment.
    ///
    /// Absent — every ordinary run — [`the_glib_fatal_probe_child`] does nothing at
    /// all, which is what keeps a test that deliberately kills its process out of the
    /// suite proper.
    const GLIB_FATAL_PROBE_REPORT: &str = "SCRIBOBULATE_GLIB_FATAL_PROBE_REPORT";

    /// The message the probe's fatal log carries, so the parent can tell "GLib reached
    /// its fatal path" from "the child died of something else".
    const GLIB_FATAL_PROBE_MESSAGE: &std::ffi::CStr = c"scribobulate glib-fatal probe";

    /// **The arrival half: a real GLib fatal message, in a real process.**
    ///
    /// [`FATAL_SIGNALS`]'s inclusion of `SIGTRAP` rests on a claim about *somebody
    /// else's* library — that `_g_log_abort` breakpoints rather than aborts on unix.
    /// The enumeration test above proves only the coverage half (the handler takes the
    /// signal); it would pass identically if that claim were false and no GLib death
    /// ever landed there. So this drives `G_LOG_LEVEL_ERROR` through GLib itself and
    /// asserts where the process ends up.
    ///
    /// **Why a re-executed child rather than a fork.** The other tests here fork and
    /// raise, because a forked child of a multi-threaded process may not allocate — and
    /// `g_log` allocates and takes locks, so a fork would risk a *hang* rather than a
    /// failure. `Command` gives a fresh, single-threaded process instead, at the cost of
    /// re-entering libtest by name.
    ///
    /// **It cannot pass vacuously.** A wrong test name makes libtest run nothing and
    /// exit 0, which fails the signal assertion; a child that died for an unrelated
    /// reason fails the stderr assertion; and removing `SIGTRAP` from [`FATAL_SIGNALS`]
    /// leaves the child dying by SIGTRAP with **no report**, which is the mutation this
    /// test exists to catch (confirmed by doing it).
    ///
    /// MEASURED, GLib 2.72.4: `g_error` and a `G_DEBUG=fatal-warnings` warning exit 133
    /// (`SIGTRAP`); `g_assert_not_reached()` exits 134 (`SIGABRT`) via the same
    /// function's other branch. If a future GLib aborts instead, this fails and says
    /// so — the enumeration is then merely wider than it needs to be, not wrong.
    #[test]
    fn a_glib_fatal_message_dies_by_a_signal_this_handler_takes() {
        if !cfg!(all(target_os = "linux", target_env = "gnu")) {
            eprintln!(
                "SKIPPED [TDD 21.4 GLib-fatal arrival]: needs a re-executed child and a \
                 glibc handler. NOT verified by this run."
            );
            return;
        }
        use std::os::unix::process::ExitStatusExt as _;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash-glib-fatal.log");
        // The lib test binary names its tests without the crate name.
        let child_test = format!(
            "{}::the_glib_fatal_probe_child",
            module_path!()
                .strip_prefix(concat!(env!("CARGO_CRATE_NAME"), "::"))
                .unwrap_or(module_path!())
        );
        let output = std::process::Command::new(
            std::env::current_exe().expect("the running test binary's own path"),
        )
        .args(["--exact", &child_test, "--nocapture"])
        .env(GLIB_FATAL_PROBE_REPORT, &path)
        .output()
        .expect("re-executing the test binary");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(GLIB_FATAL_PROBE_MESSAGE.to_str().unwrap()),
            "the child never emitted the fatal GLib message, so it died of something \
             else and proves nothing:\n{stderr}"
        );
        assert_eq!(
            output.status.signal(),
            Some(libc::SIGTRAP),
            "a GLib FATAL message no longer lands on SIGTRAP (status {:?}) — \
             FATAL_SIGNALS was widened on the premise that it does:\n{stderr}",
            output.status
        );

        let text = std::fs::read_to_string(&path)
            .expect("a GLib fatal death left no crash report — the whole point of taking SIGTRAP");
        assert!(text.contains("signal: SIGTRAP"), "{text}");
        assert!(
            text.contains("a GLib fatal message is about to fire"),
            "{text}"
        );
    }

    /// The doomed half of [`a_glib_fatal_message_dies_by_a_signal_this_handler_takes`]:
    /// arm the handler, then die the way GTK dies.
    ///
    /// A no-op unless [`GLIB_FATAL_PROBE_REPORT`] is set, which only that test does.
    /// No [`ArmedHandler`] here on purpose — this process is about to be killed by the
    /// very handler it installs, so there is nothing to restore and no later test to
    /// protect (ScrAP-265's rule is about a *surviving* process).
    ///
    /// The fatal message goes through `glib::ffi::g_log` rather than a `g_*!` macro:
    /// POLICY's logging section bans those for application logging, and spelling the
    /// call out makes it obvious this is a deliberate provocation of GLib's fatal path
    /// rather than a stray log line.
    #[test]
    fn the_glib_fatal_probe_child() {
        let Ok(path) = std::env::var(GLIB_FATAL_PROBE_REPORT) else {
            return;
        };
        let ring: &'static Ring = Box::leak(Box::new(Ring::new()));
        ring.record("2026-08-11T10:00:00.000Z INFO  scribobulate::probe: a GLib fatal message is about to fire");
        install(
            std::path::Path::new(&path),
            "scribobulate 0.1.0 (glib-fatal probe)\npid: probe\n",
            ring,
        );

        // SAFETY: a NUL-terminated literal format and argument; `g_log` at
        // `G_LOG_LEVEL_ERROR` is fatal by definition and does not return.
        unsafe {
            glib::ffi::g_log(
                std::ptr::null(),
                glib::ffi::G_LOG_LEVEL_ERROR,
                c"%s".as_ptr(),
                GLIB_FATAL_PROBE_MESSAGE.as_ptr(),
            );
        }
        unreachable!("a G_LOG_LEVEL_ERROR message returned instead of killing the process");
    }

    /// **H-2, the cross-writer half.** The fatal-signal handler must not destroy the
    /// panic hook's report.
    ///
    /// This is the normal way a GTK app reaches `SIGABRT`, not an exotic one: a panic
    /// inside a gtk-rs `extern "C"` callback cannot unwind across the C frame (glib
    /// 0.21.5 installs no `catch_unwind` there), so the panic hook writes a full report
    /// naming the panic and its location, the runtime aborts, and this handler fires on
    /// the resulting `SIGABRT` — over the same pre-computed path. While `open_report`
    /// passed `O_TRUNC`, the surviving artefact was the one that said only
    /// `signal: SIGABRT`, and the panic message, source location and backtrace that
    /// actually identified the bug were gone.
    ///
    /// The panic-hook report is written in the PARENT, before the fork, for the same
    /// reason `install` is: it allocates, and allocating in a forked child of a
    /// multi-threaded process can deadlock on a `malloc` lock held at fork time. The
    /// child does nothing but raise. Mutation guard: restore `O_TRUNC` in
    /// [`open_report`] and every "the signal handler destroyed …" assertion fails.
    ///
    /// Runtime-skipped rather than `#[cfg]`-gated, matching the sibling above. I wrote
    /// this test with a `#[cfg(all(target_os = "linux", target_env = "gnu"))]` in the
    /// very round whose M-6 finding is that a `cfg` on a test DELETES it rather than
    /// skipping it — the merge with the macOS seat's fix is what surfaced it, since
    /// their change is that rule applied to the three tests beside this one.
    #[test]
    fn a_fatal_signal_does_not_destroy_the_panic_reports_evidence() {
        if !cfg!(all(target_os = "linux", target_env = "gnu")) {
            eprintln!(
                "SKIPPED [TDD 21.12 / QA R5 H-2 crash-report preservation across a \
                 fatal signal]: needs a forked child raising a real SIGABRT against a \
                 glibc handler. NOT verified by this run."
            );
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash-abort.log");
        let ring: &'static Ring = Box::leak(Box::new(Ring::new()));
        ring.record("2026-07-31T12:00:00.000Z INFO  scribobulate::window: the last thing it did");

        let _armed = ArmedHandler::arm(
            &path,
            "scribobulate 0.1.0 (test, release)\npid: test\n",
            ring,
        );

        // What the panic hook writes on its way to `abort()` — the informative report.
        super::super::report::write_report(
            &path,
            "panic",
            "scribobulate 0.1.0 (test, release)\npid: test\n",
            "panic: index out of bounds: the len is 3 but the index is 7\n\
             location: src/preview/render.rs:118",
            ring,
            Some("frame #0 the_panicking_frame"),
        )
        .expect("the panic hook's report");

        let status = a_child_raises(libc::SIGABRT);
        assert!(
            libc::WIFSIGNALED(status),
            "child exited normally ({status})"
        );
        assert_eq!(libc::WTERMSIG(status), libc::SIGABRT);

        let text = std::fs::read_to_string(&path).expect("no report at all");
        for lost in [
            "panic: index out of bounds: the len is 3 but the index is 7",
            "location: src/preview/render.rs:118",
            "frame #0 the_panicking_frame",
        ] {
            assert!(
                text.contains(lost),
                "the signal handler destroyed the panic report's {lost:?}:\n{text}"
            );
        }
        // …and the abort is recorded after it, not instead of it.
        assert!(text.contains("signal: SIGABRT"), "{text}");
        assert!(
            text.find("panic: index out of bounds").unwrap()
                < text.find("signal: SIGABRT").unwrap(),
            "the faults must read oldest-first:\n{text}"
        );
    }

    /// **The handler must not outlive the test that armed it.**
    ///
    /// [`install`] publishes into process-wide state and has no uninstall, so before
    /// [`ArmedHandler`] existed the two tests above left the *whole rest of the suite*
    /// running with this module's handler armed and this module's leaked `Ring`
    /// published. What that cost was a coverage run — POLICY's build-pipeline step 6 —
    /// dying by SIGSEGV and printing a complete, plausible Scribobulate crash report
    /// whose breadcrumbs were the fixture two tests up. The report named the wrong
    /// process, the wrong activity, and an application defect that did not exist, and
    /// the first thing anyone reading it concluded was that the app had crashed.
    ///
    /// **Both halves are here on purpose** (ScrAP-217): "no report was written" and
    /// "this harness cannot write reports at all" produce identical output, so the
    /// *armed* half is the positive control that gives the disarmed half a meaning.
    ///
    /// **And both restorations are asserted separately** (ScrAP-254): putting the
    /// disposition back is on its own sufficient to make the report half pass, so the
    /// alternate stack — the half Rust's own stack-overflow handler needs back, and the
    /// half that decides whether a later overflow reads as "has overflowed its stack"
    /// or as a process-wide SIGSEGV — carries its own assertion rather than riding
    /// along on a sufficient sibling.
    ///
    /// **Mutation guard, both ways:** make [`ArmedHandler::disarm`] return early and
    /// the disposition, stack and report assertions all fail; delete only its
    /// `sigaltstack` line and the stack assertion fails alone.
    #[test]
    fn the_fatal_handler_does_not_outlive_the_test_that_armed_it() {
        if !cfg!(all(target_os = "linux", target_env = "gnu")) {
            eprintln!(
                "SKIPPED [ScrAP-265 fatal-handler test hygiene]: needs a forked child \
                 raising a real SIGSEGV against a glibc handler. NOT verified by this \
                 run."
            );
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("must-not-grow.log");
        let ring: &'static Ring = Box::leak(Box::new(Ring::new()));
        ring.record("2026-08-09T09:00:00.000Z INFO  scribobulate::app::open: opened /tmp/a.md");

        let mut armed = ArmedHandler::arm(
            &path,
            "scribobulate 0.1.0 (test, release)\npid: test\n",
            ring,
        );

        // The positive control: while armed, a fatal signal DOES produce a report.
        assert_eq!(
            current_sigsegv_handler(),
            on_fatal as *const () as libc::sighandler_t,
            "arming did not take SIGSEGV — the rest of this test would pass vacuously"
        );
        a_child_raises(libc::SIGSEGV);
        let armed_len = std::fs::metadata(&path)
            .expect("the armed handler wrote no report — the control is broken, not the check")
            .len();
        assert!(armed_len > 0, "the armed handler wrote an empty report");

        armed.disarm(); // still holding the install lock, so nothing else can re-arm

        assert_eq!(
            current_sigsegv_handler(),
            armed.displaced_sigsegv_handler(),
            "SIGSEGV still points somewhere this module put it — Rust's stack-overflow \
             handler is displaced for the rest of the suite, so a later overflow dies \
             as a process-wide SIGSEGV instead of naming itself"
        );
        assert_eq!(
            current_alternate_stack(),
            armed.displaced_alternate_stack(),
            "this thread is still on the handler's leaked alternate stack"
        );

        // …and behaviourally: the same fault that grew the report a moment ago now
        // leaves it untouched. `O_APPEND` means any run of the handler would show up
        // as growth, so an unchanged length is the whole property.
        a_child_raises(libc::SIGSEGV);
        assert_eq!(
            std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
            armed_len,
            "a fatal signal AFTER this test finished with the handler still wrote a \
             crash report — with this test's breadcrumbs in it"
        );
    }

    #[test]
    fn this_process_has_executable_mappings_to_report() {
        if !cfg!(target_os = "linux") {
            eprintln!(
                "SKIPPED [TDD 21.8 executable-mappings enumeration]: needs \
                 /proc/self/maps, which does not exist on this platform. \
                 NOT verified by this run."
            );
            return;
        }
        // Guards the field offset above against a `/proc` format assumption that
        // was only ever checked against a remembered example.
        let maps = std::fs::read_to_string("/proc/self/maps").unwrap();
        let executable = maps
            .lines()
            .filter(|line| is_executable_mapping(line.as_bytes()))
            .count();
        assert!(
            executable > 0,
            "no executable mapping recognised in:\n{maps}"
        );
        assert!(
            executable < maps.lines().count(),
            "every mapping matched — the filter is not filtering"
        );
    }
}
