//! What a native crash looks like from inside the process — the source the Windows crash
//! report needs, because Windows has no POSIX signals.
//!
//! On unix a fatal fault arrives as a signal and `forensics::signal` takes it. Here it
//! arrives one of two ways, and before this module both reached Windows Error Reporting
//! and nothing else: a real case, `0xc0000005` inside `gtk-4-1.dll` on a user's machine,
//! left a WER entry and no report of ours.
//!
//! | Death | How it reaches the process | Taken by |
//! |---|---|---|
//! | Access violation, illegal instruction, breakpoint, stack overflow, divide by zero, … — on any thread, in any module | an SEH exception no frame handled | [`SetUnhandledExceptionFilter`] |
//! | `abort()` — including **every GLib fatal message** (`g_error`, `g_assert_not_reached`, any level made fatal by `G_DEBUG`) | the CRT raises `SIGABRT`, then fails fast | the CRT's `signal(SIGABRT)` |
//!
//! Supplies the facts and the raw I/O only — the report's shape is
//! `forensics::exception`'s (POLICY § Platform seams).
//!
//! # Why the unhandled-exception filter and not a vectored handler
//!
//! A vectored handler sees every **first-chance** exception, before any frame has had
//! its say — including the ones code raises and handles on purpose. MEASURED (Windows 10
//! 19045, no GTK loaded): `IsBadReadPtr` on a bad address, `OutputDebugStringA` with no
//! debugger attached, and a Rust panic caught by `catch_unwind` raised **three**
//! exceptions a vectored handler saw, while the process carried on unharmed. Each would
//! have been a crash report naming a crash that did not happen. The unhandled filter is
//! called only once every frame has declined, which is the definition of a death; none of
//! those three reached it.
//!
//! **Nothing replaces it behind us, measured.** Every gvsbuild DLL imports
//! `SetUnhandledExceptionFilter`, which reads like a hazard. It is the MSVC runtime's
//! `/GS` failure path, and modern Windows takes `__fastfail` there instead, so the import
//! is never called. GLib 2.88.1's own handler (`g_crash_handler_win32_init`) is a
//! *vectored* one, registered only when `G_DEBUGGER` is set, and it does not touch this
//! filter. The proof that the filter survives a full GTK start-up is
//! `forensics::exception`'s fault injected into a running release build.
//!
//! # Why `abort()` needs its own door, and how a GLib fatal message dies here
//!
//! The CRT's `abort()` raises no exception at all: it calls a `SIGABRT` handler if one is
//! installed, then ends the process with `__fastfail(FAST_FAIL_FATAL_APP_EXIT)`, which no
//! exception filter can see. MEASURED: exit `0xc0000409`, our handler having run first.
//!
//! That is how GLib dies on Windows — **not** by a breakpoint. GLib 2.88.1's `g_abort()`
//! breakpoints only `if (IsDebuggerPresent ())` and otherwise calls `abort()`
//! (`glib/gutils.c`); `_g_log_abort` routes every fatal message there. A breakpoint
//! exception is still taken, by the filter, for the case a library does raise one.
//!
//! This works because the application and every gvsbuild DLL share **one** CRT,
//! `ucrtbase.dll` (all import it dynamically), and `signal()`'s table lives there. A DLL
//! linked against a static CRT would keep its own table, and its `abort()` would bypass
//! this handler.
//!
//! # What cannot be caught, stated rather than papered over
//!
//! * **`__fastfail`** ends the process in the kernel with no user-mode code running. It is
//!   how Rust's `process::abort` dies, and so `panic = "abort"` and "panic in a function
//!   that cannot unwind" — by which point `report::install_panic_hook` has already
//!   written that panic's report. It is also how the MSVC runtime reports a `/GS` cookie
//!   failure, an invalid CRT parameter, a pure virtual call and a CFG violation, and how
//!   the NT heap reports corruption it detects.
//! * **A debugger attached**: Windows does not call the filter; the debugger owns the
//!   exception.
//! * **`TerminateProcess`**, from inside or out, is not a crash and runs nothing.
//! * **A stack overflow with less room than the report needs.** MEASURED: the filter runs
//!   with ~24 KiB to spare on a thread Rust started (std reserves it with
//!   `SetThreadStackGuarantee`) and ~11.7 KiB on a raw `CreateThread` thread, as GLib's
//!   are. The report is written in under 8 KiB of stack — a test overflows a raw thread
//!   to hold that. A thread that already ate its guarantee writes nothing.
//! * **A fault while writing the report.** What is on disk stays (TDD 21.6), and the
//!   process then dies of the *second* fault's code, not the first.
//!
//! # Termination is unchanged
//!
//! The filter reports, then hands the exception on — to whichever filter it displaced,
//! else back to Windows with `EXCEPTION_CONTINUE_SEARCH` — so WER still runs and the exit
//! code is the exception code, exactly as without it (GEP-35). The `SIGABRT` handler
//! returns, and the CRT's own `__fastfail` follows. MEASURED both ways, WER's
//! `Application Error` event included. One case to know: a fault inside a window procedure
//! that the kernel called back into ends the process with
//! `STATUS_FATAL_USER_CALLBACK_EXCEPTION` (`0xc000041d`) rather than the fault's own code.
//! The filter still sees, and reports, the real code; the exit code is Windows' own, with
//! or without the filter.
//!
//! # The report path neither allocates nor locks
//!
//! A crash can land inside the heap with its lock held, or inside the loader with its
//! lock held — the `DllMain` case. So nothing here takes either:
//!
//! * the module list comes from `VirtualQuery` over the address space, and each module's
//!   path from `NtQueryVirtualMemory`, into stack buffers — system calls, not the loader's
//!   module list (which `GetModuleFileNameW` and `EnumProcessModules` walk under its lock);
//! * the clock is `GetSystemTimeAsFileTime`, a read of shared memory;
//! * the backtrace is `RtlLookupFunctionEntry` + `RtlVirtualUnwind`. These consult the
//!   loader's function table under a *shared* lock — the same lookups, over the same
//!   frames, Windows' own exception dispatch made to reach this filter at all. If that
//!   lock could not be taken, the filter would never have been called. The backtrace is
//!   still written last, because a corrupt stack is what it reads.
//!
//! Gated by its parent's `#[cfg(windows)]`, like the rest of the Win32 seam.

use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use super::CloseHandle;

/// `EXCEPTION_CONTINUE_SEARCH`: decline, so the next filter — finally WER — runs.
const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
/// `EXCEPTION_ACCESS_VIOLATION` — its parameters name the address and the access.
pub(crate) const EXCEPTION_ACCESS_VIOLATION: u32 = 0xC000_0005;
/// `EXCEPTION_IN_PAGE_ERROR` — parameters shaped like an access violation's.
pub(crate) const EXCEPTION_IN_PAGE_ERROR: u32 = 0xC000_0006;
/// `STD_ERROR_HANDLE`, i.e. `(DWORD)-12`.
const STD_ERROR_HANDLE: u32 = -12i32 as u32;
/// `INVALID_HANDLE_VALUE`.
const INVALID_HANDLE_VALUE: isize = -1;
/// `FILE_APPEND_DATA` — every write lands at the end, whoever else wrote the file.
const FILE_APPEND_DATA: u32 = 0x0004;
const FILE_SHARE_READ: u32 = 0x0001;
const FILE_SHARE_WRITE: u32 = 0x0002;
/// `OPEN_ALWAYS` — create, or open what the panic hook already wrote.
const OPEN_ALWAYS: u32 = 4;
const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
/// `MEM_IMAGE` — a region mapped from an executable image, i.e. part of a module.
const MEM_IMAGE: u32 = 0x0100_0000;
/// `MemoryMappedFilenameInformation`, the `NtQueryVirtualMemory` class naming a
/// mapping's file.
const MEMORY_MAPPED_FILENAME_INFORMATION: u32 = 2;
/// The current-process pseudo-handle, which is what `GetCurrentProcess` returns.
const CURRENT_PROCESS: *mut c_void = -1isize as *mut c_void;
/// How long a second crashing thread waits for the first to finish its report before
/// letting the process die anyway, in `Sleep` steps of [`WAIT_STEP_MS`].
const WAIT_STEPS: u32 = 500;
const WAIT_STEP_MS: u32 = 10;

/// `EXCEPTION_RECORD`.
#[repr(C)]
struct ExceptionRecord {
    code: u32,
    flags: u32,
    nested: *mut ExceptionRecord,
    address: *mut c_void,
    parameter_count: u32,
    parameters: [usize; 15],
}

/// `EXCEPTION_POINTERS`.
#[repr(C)]
pub(crate) struct ExceptionPointers {
    record: *mut ExceptionRecord,
    context: *mut c_void,
}

type TopLevelFilter = unsafe extern "system" fn(*mut ExceptionPointers) -> i32;

/// `MEMORY_BASIC_INFORMATION` (64-bit layout; `repr(C)` supplies the padding).
#[repr(C)]
struct MemoryBasicInformation {
    base: usize,
    allocation_base: usize,
    allocation_protect: u32,
    partition_id: u16,
    region_size: usize,
    state: u32,
    protect: u32,
    kind: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn SetUnhandledExceptionFilter(filter: Option<TopLevelFilter>) -> Option<TopLevelFilter>;
    fn GetCurrentThreadId() -> u32;
    fn Sleep(milliseconds: u32);
    fn GetStdHandle(which: u32) -> *mut c_void;
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *mut c_void,
        disposition: u32,
        flags: u32,
        template: *mut c_void,
    ) -> *mut c_void;
    fn WriteFile(
        file: *mut c_void,
        buffer: *const u8,
        len: u32,
        written: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
    fn GetSystemTimeAsFileTime(time: *mut u64);
    fn VirtualQuery(address: *const c_void, info: *mut MemoryBasicInformation, len: usize)
        -> usize;
}

#[link(name = "ntdll")]
extern "system" {
    fn NtQueryVirtualMemory(
        process: *mut c_void,
        address: *const c_void,
        class: u32,
        buffer: *mut c_void,
        len: usize,
        returned: *mut usize,
    ) -> i32;
}

// ── What died, and how ──────────────────────────────────────────────────────────

/// How the process is dying.
pub(crate) enum Cause {
    /// An SEH exception no frame handled.
    Exception {
        code: u32,
        /// The address the faulting instruction tried to touch, for an access violation
        /// or in-page error; otherwise the faulting instruction itself — the same meaning
        /// unix gives `si_addr`.
        fault_address: usize,
        /// For an access violation or in-page error, what the instruction tried to do.
        access: Option<Access>,
        instruction_pointer: usize,
    },
    /// `abort()`, delivered as the CRT's `SIGABRT`. A call, not a fault: there is no
    /// faulting instruction, and the backtrace runs through `abort()` to its caller.
    Abort,
}

/// What an access violation was doing, from its first parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Access {
    Read,
    Write,
    /// Data-execution prevention: an instruction fetch from non-executable memory.
    Execute,
    Unknown(usize),
}

impl Access {
    fn from_parameter(value: usize) -> Self {
        match value {
            0 => Self::Read,
            1 => Self::Write,
            8 => Self::Execute,
            other => Self::Unknown(other),
        }
    }
}

/// One death, as the report writer sees it. Its register context is in [`FAULT`].
///
/// Only [`on_unhandled`] and [`on_abort`] build one, and only after [`claim`] made their
/// thread the reporter — which is what makes the slots below safe to touch through it.
pub(crate) struct Crash {
    pub(crate) cause: Cause,
    pub(crate) thread_id: u32,
}

impl Crash {
    /// Every frame's address, innermost first, up to `limit` of them.
    pub(crate) fn for_each_frame(&self, limit: usize, visit: impl FnMut(usize)) {
        // SAFETY: a `Crash` exists only on the claimed reporter thread (see the type), so
        // nothing else touches either slot; the two never alias.
        unsafe {
            let walk = WALK.get();
            std::ptr::copy_nonoverlapping(FAULT.get(), walk, 1);
            unwind(&mut *walk, limit, visit);
        }
    }
}

/// Room for a register context that is **not on the dying thread's stack**.
///
/// A stack overflow leaves the faulting thread very little stack, and a `CONTEXT` is
/// 1232 bytes — two of them (the fault's, and a copy to unwind) were the difference
/// between a report and a second fault. MEASURED: the debug test build overflowing a raw
/// thread died of `0xc0000005` inside the handler with both on the stack, and reported
/// with them here. Static, not heap: the heap may be what is broken.
struct Slot(std::cell::UnsafeCell<Context>);

// SAFETY: written and read only by the one thread `claim` makes the reporter.
unsafe impl Sync for Slot {}

impl Slot {
    fn get(&self) -> *mut Context {
        self.0.get()
    }
}

/// The dying thread's registers, as the fault left them (or as `abort()` called).
static FAULT: Slot = Slot(std::cell::UnsafeCell::new(Context::ZEROED));
/// A copy of [`FAULT`] for unwinding, which rewrites the context it walks.
static WALK: Slot = Slot(std::cell::UnsafeCell::new(Context::ZEROED));

/// The report writer, published once by [`install`].
type OnCrash = fn(&Crash);
static ON_CRASH: AtomicUsize = AtomicUsize::new(0);
/// Whatever filter [`install`] displaced, as an address, or 0.
static DISPLACED: AtomicUsize = AtomicUsize::new(0);

/// Arm both doors. `on_crash` runs at most once per process, on the dying thread, and
/// must neither allocate nor lock.
///
/// No uninstall, for `forensics::signal`'s reason: a production process wants this for
/// its whole life. Tests that would leave it armed run it in a child process instead.
pub(crate) fn install(on_crash: OnCrash) {
    ON_CRASH.store(on_crash as usize, Ordering::Release);
    // SAFETY: `on_unhandled` has the `LPTOP_LEVEL_EXCEPTION_FILTER` signature, and the
    // displaced filter is stored before anything can be dispatched to ours.
    let displaced = unsafe { SetUnhandledExceptionFilter(Some(on_unhandled)) };
    DISPLACED.store(displaced.map_or(0, |f| f as usize), Ordering::Release);
    // SAFETY: `on_abort` has the CRT's `void (int)` handler signature.
    unsafe {
        libc::signal(libc::SIGABRT, on_abort as *const () as libc::sighandler_t);
    }
}

unsafe extern "system" fn on_unhandled(pointers: *mut ExceptionPointers) -> i32 {
    match claim() {
        Claim::Nested => return EXCEPTION_CONTINUE_SEARCH,
        Claim::First => {
            if !pointers.is_null() {
                report(crash_from_exception(&*pointers));
            }
            finish();
        }
        Claim::Second | Claim::Done => {}
    }
    hand_on(pointers)
}

/// Pass the exception to whatever filter was there before ours, so termination is what
/// it was without us.
unsafe fn hand_on(pointers: *mut ExceptionPointers) -> i32 {
    match DISPLACED.load(Ordering::Acquire) {
        0 => EXCEPTION_CONTINUE_SEARCH,
        // SAFETY: a non-zero value is the `TopLevelFilter` `install` stored.
        filter => std::mem::transmute::<usize, TopLevelFilter>(filter)(pointers),
    }
}

/// The CRT's `SIGABRT` handler. It returns, and `abort()` then fails fast as it would have
/// with no handler at all.
extern "C" fn on_abort(_signal: libc::c_int) {
    if let Claim::First = claim() {
        // SAFETY: this thread is the claimed reporter, so the slot is its alone.
        unsafe { (*FAULT.get()).capture() };
        report(Crash {
            cause: Cause::Abort,
            // SAFETY: no preconditions.
            thread_id: unsafe { GetCurrentThreadId() },
        });
        finish();
    }
}

fn report(crash: Crash) {
    let on_crash = ON_CRASH.load(Ordering::Acquire);
    if on_crash != 0 {
        // SAFETY: the only non-zero value ever stored is an `OnCrash`.
        let on_crash = unsafe { std::mem::transmute::<usize, OnCrash>(on_crash) };
        on_crash(&crash);
    }
}

/// # Safety
/// The caller is the claimed reporter thread, and `pointers` is what Windows passed it.
unsafe fn crash_from_exception(pointers: &ExceptionPointers) -> Crash {
    if !pointers.context.is_null() {
        (*FAULT.get()).copy_from(pointers.context);
    }
    let cause = match pointers.record.as_ref() {
        Some(record) => {
            let instruction_pointer = record.address as usize;
            let faulted_on_data = matches!(
                record.code,
                EXCEPTION_ACCESS_VIOLATION | EXCEPTION_IN_PAGE_ERROR
            ) && record.parameter_count >= 2;
            let (fault_address, access) = if faulted_on_data {
                (
                    record.parameters[1],
                    Some(Access::from_parameter(record.parameters[0])),
                )
            } else {
                (instruction_pointer, None)
            };
            Cause::Exception {
                code: record.code,
                fault_address,
                access,
                instruction_pointer,
            }
        }
        None => Cause::Exception {
            code: 0,
            fault_address: 0,
            access: None,
            instruction_pointer: 0,
        },
    };
    Crash {
        cause,
        thread_id: GetCurrentThreadId(),
    }
}

// ── One report per process ─────────────────────────────────────────────────────────

const IDLE: u32 = 0;
const REPORTING: u32 = 1;
const DONE: u32 = 2;
static STATE: AtomicU32 = AtomicU32::new(IDLE);
static REPORTER: AtomicU32 = AtomicU32::new(0);

enum Claim {
    /// This thread writes the report.
    First,
    /// This thread faulted *while* writing the report. Give up at once: what reached the
    /// file is what the reader gets (TDD 21.6).
    Nested,
    /// Another thread was writing a report; it has finished, or been given long enough.
    Second,
    /// A report was already written; nothing more to add.
    Done,
}

/// Decide who writes. Two threads can die at once; the second waits for the first rather
/// than handing the process to WER mid-report, which would truncate it — but not
/// forever, because a report that hangs must not turn a crash into a hang.
fn claim() -> Claim {
    // SAFETY: no preconditions.
    let me = unsafe { GetCurrentThreadId() };
    match STATE.compare_exchange(IDLE, REPORTING, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => {
            REPORTER.store(me, Ordering::Release);
            Claim::First
        }
        Err(REPORTING) if REPORTER.load(Ordering::Acquire) == me => Claim::Nested,
        Err(REPORTING) => {
            for _ in 0..WAIT_STEPS {
                if STATE.load(Ordering::Acquire) != REPORTING {
                    break;
                }
                // SAFETY: no preconditions.
                unsafe { Sleep(WAIT_STEP_MS) };
            }
            Claim::Second
        }
        Err(_) => Claim::Done,
    }
}

fn finish() {
    STATE.store(DONE, Ordering::Release);
}

// ── Raw output ──────────────────────────────────────────────────────────────────────

/// The report file, opened for append, or stderr when it cannot be — so a crash is never
/// completely silent.
pub(crate) struct ReportFile {
    handle: *mut c_void,
    owned: bool,
}

impl ReportFile {
    /// Open `path` (UTF-16, NUL-terminated) for append, creating it if absent.
    ///
    /// Append, never truncate, for `forensics::signal`'s reason (QA round 5, H-2): the
    /// panic hook may already have written this path, and its report is the better one.
    ///
    /// **Owner-only comes from the directory.** The file is created with no security
    /// descriptor, so it takes the inheritable ACEs of the state directory, which
    /// `privacy::create_private_directory` makes owner-only and protected — the
    /// containing-directory mechanism TDD 21.12 names for a platform without mode bits.
    pub(crate) fn open_append(path: &[u16]) -> Self {
        // SAFETY: `path` is NUL-terminated (the caller's contract, checked below) and
        // outlives the call; every pointer argument is null or live.
        let handle = if path.last() == Some(&0) {
            unsafe {
                CreateFileW(
                    path.as_ptr(),
                    FILE_APPEND_DATA,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    std::ptr::null_mut(),
                    OPEN_ALWAYS,
                    FILE_ATTRIBUTE_NORMAL,
                    std::ptr::null_mut(),
                )
            }
        } else {
            INVALID_HANDLE_VALUE as *mut c_void
        };
        if handle as isize == INVALID_HANDLE_VALUE || handle.is_null() {
            Self {
                // SAFETY: no preconditions; a missing stderr yields null, and writes to it
                // then fail quietly.
                handle: unsafe { GetStdHandle(STD_ERROR_HANDLE) },
                owned: false,
            }
        } else {
            Self {
                handle,
                owned: true,
            }
        }
    }

    /// Write all of `bytes`, retrying short writes. A failed write ends the report there.
    pub(crate) fn write(&self, mut bytes: &[u8]) {
        while !bytes.is_empty() {
            let chunk = bytes.len().min(u32::MAX as usize) as u32;
            let mut written: u32 = 0;
            // SAFETY: writing `chunk` bytes from a live slice to a handle this owns or
            // borrowed from the process.
            let ok = unsafe {
                WriteFile(
                    self.handle,
                    bytes.as_ptr(),
                    chunk,
                    &mut written,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 || written == 0 {
                return;
            }
            bytes = &bytes[written as usize..];
        }
    }
}

impl Drop for ReportFile {
    fn drop(&mut self) {
        if self.owned {
            // SAFETY: a handle `CreateFileW` returned to this value and nothing else.
            unsafe { CloseHandle(self.handle) };
        }
    }
}

/// The wall clock in Unix milliseconds, from shared memory — no lock, no allocation.
pub(crate) fn now_unix_millis() -> i64 {
    /// 100 ns ticks between 1601-01-01 and 1970-01-01.
    const UNIX_EPOCH_TICKS: i64 = 116_444_736_000_000_000;
    let mut ticks: u64 = 0;
    // SAFETY: a live out-parameter of the right size.
    unsafe { GetSystemTimeAsFileTime(&mut ticks) };
    (ticks as i64 - UNIX_EPOCH_TICKS) / 10_000
}

// ── Modules, without the loader ─────────────────────────────────────────────────────

/// One loaded image: `base..end`, as the address space holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Module {
    pub(crate) base: usize,
    pub(crate) end: usize,
}

fn query(address: usize) -> Option<MemoryBasicInformation> {
    let mut info = MemoryBasicInformation {
        base: 0,
        allocation_base: 0,
        allocation_protect: 0,
        partition_id: 0,
        region_size: 0,
        state: 0,
        protect: 0,
        kind: 0,
    };
    // SAFETY: a live out-parameter of the size passed; any address may be queried.
    let got = unsafe {
        VirtualQuery(
            address as *const c_void,
            &mut info,
            std::mem::size_of::<MemoryBasicInformation>(),
        )
    };
    (got != 0).then_some(info)
}

/// The module `address` falls in, or `None` if it is not inside a loaded image.
pub(crate) fn module_containing(address: usize) -> Option<Module> {
    let first = query(address)?;
    if first.kind != MEM_IMAGE || first.allocation_base == 0 {
        return None;
    }
    let base = first.allocation_base;
    Some(Module {
        base,
        end: image_end(base),
    })
}

/// Where the image starting at `base` ends: the end of the last region still belonging to
/// that allocation.
fn image_end(base: usize) -> usize {
    let mut end = base;
    while let Some(region) = query(end) {
        if region.allocation_base != base || region.region_size == 0 {
            break;
        }
        end = region.base + region.region_size;
    }
    end
}

/// Every loaded image, in address order — the Windows `/proc/self/maps`, read by walking
/// the address space region by region.
pub(crate) fn for_each_module(mut visit: impl FnMut(Module)) {
    let mut address = 0usize;
    while let Some(region) = query(address) {
        if region.kind == MEM_IMAGE && region.allocation_base == region.base {
            let module = Module {
                base: region.base,
                end: image_end(region.base),
            };
            visit(module);
            address = module.end.max(region.base + region.region_size);
        } else {
            address = region.base + region.region_size;
        }
        if region.region_size == 0 || address <= region.base {
            break;
        }
    }
}

/// Capacity, in UTF-16 units, of [`ModulePath`] — a long-path-aware NT path is rarely
/// longer, and one that is truncates rather than failing the report.
const MODULE_PATH_UNITS: usize = 520;

/// The file a module was mapped from, as an NT path
/// (`\Device\HarddiskVolume3\…\gtk-4-1.dll`), held on the stack.
///
/// An NT path rather than a drive-letter one because turning one into the other means
/// asking the mount manager, which is not a call to make from here. The file name, which
/// is what a reader matches against, is the same in both.
pub(crate) struct ModulePath {
    // `UNICODE_STRING` (16 bytes) followed by the characters, 8-aligned as the call
    // requires.
    buffer: [u64; 2 + MODULE_PATH_UNITS / 4],
    units: usize,
}

impl ModulePath {
    pub(crate) fn of(module: Module) -> Self {
        let mut path = Self {
            buffer: [0; 2 + MODULE_PATH_UNITS / 4],
            units: 0,
        };
        let mut returned = 0usize;
        // SAFETY: the buffer is live, 8-aligned and its byte size is passed; the
        // pseudo-handle names this process.
        let status = unsafe {
            NtQueryVirtualMemory(
                CURRENT_PROCESS,
                module.base as *const c_void,
                MEMORY_MAPPED_FILENAME_INFORMATION,
                path.buffer.as_mut_ptr().cast::<c_void>(),
                std::mem::size_of_val(&path.buffer),
                &mut returned,
            )
        };
        if status >= 0 {
            // `UNICODE_STRING.Length` is the first u16, in bytes.
            let bytes = (path.buffer[0] & 0xFFFF) as usize;
            path.units = (bytes / 2).min(MODULE_PATH_UNITS);
        }
        path
    }

    /// The path's UTF-16 units; empty if the query failed.
    pub(crate) fn units(&self) -> &[u16] {
        // SAFETY: the characters follow the 16-byte header (two u64s); `units` is clamped
        // to the room the buffer has after it, and any `u64` bit pattern is valid `u16`s.
        unsafe { std::slice::from_raw_parts(self.buffer.as_ptr().add(2).cast::<u16>(), self.units) }
    }

    /// Just the file name — the part after the last `\`.
    pub(crate) fn file_name(&self) -> &[u16] {
        let units = self.units();
        let start = units
            .iter()
            .rposition(|unit| *unit == u16::from(b'\\'))
            .map_or(0, |i| i + 1);
        &units[start..]
    }
}

// ── The register context, and unwinding from it ───────────────────────────────────

/// An x64 `CONTEXT`: 1232 bytes, 16-aligned, opaque but for the two registers unwinding
/// starts from. Held in a [`Slot`] on the crash path, never on the dying stack.
#[cfg(target_arch = "x86_64")]
#[repr(C, align(16))]
#[derive(Clone, Copy)]
struct Context([u8; CONTEXT_BYTES]);

#[cfg(target_arch = "x86_64")]
const CONTEXT_BYTES: usize = 0x4D0;
/// Offsets of `Rsp` and `Rip` in the x64 `CONTEXT` (`winnt.h`).
#[cfg(target_arch = "x86_64")]
const CONTEXT_RSP: usize = 0x98;
#[cfg(target_arch = "x86_64")]
const CONTEXT_RIP: usize = 0xF8;
#[cfg(target_arch = "x86_64")]
#[link(name = "kernel32")]
extern "system" {
    fn RtlCaptureContext(context: *mut Context);
    fn RtlLookupFunctionEntry(pc: u64, image_base: *mut u64, history: *mut c_void) -> *mut c_void;
    fn RtlVirtualUnwind(
        handler_type: u32,
        image_base: u64,
        pc: u64,
        function: *mut c_void,
        context: *mut Context,
        handler_data: *mut *mut c_void,
        establisher_frame: *mut u64,
        context_pointers: *mut c_void,
    ) -> *mut c_void;
    fn GetCurrentThreadStackLimits(low: *mut usize, high: *mut usize);
}

#[cfg(target_arch = "x86_64")]
impl Context {
    const ZEROED: Self = Self([0; CONTEXT_BYTES]);

    /// # Safety
    /// `source` must point at a live `CONTEXT`.
    unsafe fn copy_from(&mut self, source: *const c_void) {
        std::ptr::copy_nonoverlapping(source.cast::<u8>(), self.0.as_mut_ptr(), CONTEXT_BYTES);
    }

    fn capture(&mut self) {
        // SAFETY: a live, correctly sized and aligned `CONTEXT`.
        unsafe { RtlCaptureContext(self) };
    }

    fn register(&self, offset: usize) -> u64 {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&self.0[offset..offset + 8]);
        u64::from_ne_bytes(bytes)
    }

    fn set_register(&mut self, offset: usize, value: u64) {
        self.0[offset..offset + 8].copy_from_slice(&value.to_ne_bytes());
    }

    #[cfg(test)]
    fn instruction_pointer(&self) -> u64 {
        self.register(CONTEXT_RIP)
    }
}

/// Walk the stack from `context`, innermost frame first, rewriting it frame by frame.
///
/// A frame with no unwind data is a leaf, or a call to a bad address (a null function
/// pointer): either way its return address is at the top of the stack, which is the rule
/// Windows' own unwinder applies. That read is bounds-checked against this thread's stack
/// first, since this is the one place a corrupt stack is dereferenced.
#[cfg(target_arch = "x86_64")]
fn unwind(context: &mut Context, limit: usize, mut visit: impl FnMut(usize)) {
    let (mut low, mut high) = (0usize, 0usize);
    // SAFETY: two live out-parameters.
    unsafe { GetCurrentThreadStackLimits(&mut low, &mut high) };

    for _ in 0..limit {
        let pc = context.register(CONTEXT_RIP);
        if pc == 0 {
            break;
        }
        visit(pc as usize);
        let mut image_base: u64 = 0;
        // SAFETY: a live out-parameter; a null history table is permitted.
        let function = unsafe { RtlLookupFunctionEntry(pc, &mut image_base, std::ptr::null_mut()) };
        if function.is_null() {
            let sp = context.register(CONTEXT_RSP) as usize;
            if sp < low || sp.saturating_add(8) > high {
                break;
            }
            // SAFETY: `sp..sp+8` lies within this thread's committed stack range.
            let return_address = unsafe { std::ptr::read_unaligned(sp as *const u64) };
            context.set_register(CONTEXT_RIP, return_address);
            context.set_register(CONTEXT_RSP, (sp + 8) as u64);
        } else {
            let mut handler_data: *mut c_void = std::ptr::null_mut();
            let mut establisher: u64 = 0;
            // SAFETY: `function` and `image_base` came from the lookup just made for
            // `pc`; the context is live and owned; the optional pointers are null or live.
            unsafe {
                RtlVirtualUnwind(
                    0, // UNW_FLAG_NHANDLER: walk, call nothing.
                    image_base,
                    pc,
                    function,
                    context,
                    &mut handler_data,
                    &mut establisher,
                    std::ptr::null_mut(),
                );
            }
        }
        let sp = context.register(CONTEXT_RSP) as usize;
        if sp < low || sp >= high {
            break;
        }
    }
}

/// Unwinding reads an architecture's own `CONTEXT`; only x64 is written here, and the
/// report says so rather than guessing.
#[cfg(not(target_arch = "x86_64"))]
#[derive(Clone, Copy)]
struct Context;

#[cfg(not(target_arch = "x86_64"))]
impl Context {
    const ZEROED: Self = Self;
    unsafe fn copy_from(&mut self, _source: *const c_void) {}
    fn capture(&mut self) {}
}

#[cfg(not(target_arch = "x86_64"))]
fn unwind(_context: &mut Context, _limit: usize, _visit: impl FnMut(usize)) {}

/// Whether this build can unwind at all, so the report can say "unavailable" rather than
/// print an empty backtrace.
pub(crate) const CAN_UNWIND: bool = cfg!(target_arch = "x86_64");

/// Ways to die, and not to, that only a Win32 call can provoke — for
/// `forensics::exception`'s child-process tests, which must not call the OS themselves.
#[cfg(test)]
pub(crate) mod provoke {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{ExceptionPointers, EXCEPTION_CONTINUE_SEARCH};

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateThread(
            security: *mut c_void,
            stack: usize,
            start: unsafe extern "system" fn(*mut c_void) -> u32,
            parameter: *mut c_void,
            flags: u32,
            id: *mut u32,
        ) -> *mut c_void;
        fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
        fn AddVectoredExceptionHandler(
            first: u32,
            handler: unsafe extern "system" fn(*mut ExceptionPointers) -> i32,
        ) -> *mut c_void;
        fn RemoveVectoredExceptionHandler(handle: *mut c_void) -> u32;
        fn IsBadReadPtr(address: *const c_void, len: usize) -> i32;
        fn OutputDebugStringA(text: *const std::ffi::c_char);
    }

    unsafe extern "system" fn trampoline(body: *mut c_void) -> u32 {
        // SAFETY: the only value passed here is the `fn()` `on_a_raw_thread` received.
        let body = std::mem::transmute::<*mut c_void, fn()>(body);
        body();
        0
    }

    /// Run `body` on a thread Rust did not start — so it has none of std's
    /// `SetThreadStackGuarantee`, like GLib's worker threads — and wait for it.
    pub(crate) fn on_a_raw_thread(body: fn()) {
        // SAFETY: `trampoline` matches `LPTHREAD_START_ROUTINE`, and its parameter is a
        // plain function pointer, which outlives the thread.
        unsafe {
            let thread = CreateThread(
                std::ptr::null_mut(),
                0,
                trampoline,
                body as *mut c_void,
                0,
                std::ptr::null_mut(),
            );
            assert!(!thread.is_null(), "CreateThread failed");
            WaitForSingleObject(thread, u32::MAX);
            super::CloseHandle(thread);
        }
    }

    static FIRST_CHANCE: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "system" fn count(_pointers: *mut ExceptionPointers) -> i32 {
        FIRST_CHANCE.fetch_add(1, Ordering::SeqCst);
        EXCEPTION_CONTINUE_SEARCH
    }

    /// Raise three exceptions that something handles — `IsBadReadPtr`'s own SEH frame, a
    /// debug string with no debugger, a Rust panic `catch_unwind` stops — and return how
    /// many a vectored handler saw. That count is the positive control: it proves the
    /// exceptions were really raised, and that a vectored handler would have reported each.
    pub(crate) fn handled_exceptions() -> usize {
        FIRST_CHANCE.store(0, Ordering::SeqCst);
        // SAFETY: `count` has the vectored-handler signature and is removed before return;
        // `IsBadReadPtr` exists to probe a bad address; the string is NUL-terminated.
        unsafe {
            let handler = AddVectoredExceptionHandler(1, count);
            assert_ne!(
                IsBadReadPtr(8 as *const c_void, 4),
                0,
                "address 8 read as valid"
            );
            OutputDebugStringA(c"scribobulate handled-exception probe".as_ptr());
            let caught = std::panic::catch_unwind(|| panic!("a panic caught on purpose"));
            assert!(caught.is_err());
            RemoveVectoredExceptionHandler(handler);
        }
        FIRST_CHANCE.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[inline(never)]
    fn a_function_in_this_binary() -> usize {
        a_function_in_this_binary as *const () as usize
    }

    /// The module lookup is VirtualQuery arithmetic, so it is checked against the one
    /// module whose identity is certain: the test binary holding this function.
    #[test]
    fn an_address_in_this_binary_resolves_to_this_binary() {
        let address = a_function_in_this_binary();
        let module = module_containing(address).expect("a code address is inside an image");
        assert!(module.base <= address && address < module.end, "{module:?}");

        let path = ModulePath::of(module);
        let name = String::from_utf16_lossy(path.file_name());
        let exe = std::env::current_exe().unwrap();
        let expected = exe.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.eq_ignore_ascii_case(&expected),
            "the mapped file name {name:?} is not the running test binary {expected:?}"
        );
        assert!(
            String::from_utf16_lossy(path.units()).starts_with("\\Device\\"),
            "not an NT path: {:?}",
            String::from_utf16_lossy(path.units())
        );
    }

    #[test]
    fn heap_and_stack_addresses_are_in_no_module() {
        let local = 0u8;
        assert_eq!(module_containing(std::ptr::addr_of!(local) as usize), None);
        let heap = Box::new(0u64);
        assert_eq!(module_containing(std::ptr::addr_of!(*heap) as usize), None);
        assert_eq!(module_containing(0), None);
    }

    /// The walk must find each module exactly once, and among them this binary and the
    /// system's own `ntdll.dll` — the floor every Windows process has.
    #[test]
    fn the_module_walk_finds_every_image_once() {
        let mut modules = Vec::new();
        for_each_module(|module| modules.push(module));
        assert!(
            modules.windows(2).all(|pair| pair[0].end <= pair[1].base),
            "{modules:?}"
        );

        let own = module_containing(a_function_in_this_binary()).unwrap();
        assert!(
            modules.contains(&own),
            "this binary missing from {modules:?}"
        );
        let names: Vec<String> = modules
            .iter()
            .map(|m| String::from_utf16_lossy(ModulePath::of(*m).file_name()).to_ascii_lowercase())
            .collect();
        assert!(names.iter().any(|n| n == "ntdll.dll"), "{names:?}");
    }

    /// The hand-written `CONTEXT` offsets are the constant that is easy to get wrong and
    /// silent when it is: a wrong `Rip` offset unwinds from garbage. So the captured `Rip`
    /// must land in this binary, and the walk must climb out through this test's caller.
    #[test]
    fn a_captured_context_unwinds_through_this_binary() {
        // Boxed: the test's own stack is not the point here, the offsets are.
        let mut context = Box::new(Context::ZEROED);
        context.capture();
        let own = module_containing(a_function_in_this_binary()).unwrap();
        let ip = context.instruction_pointer() as usize;
        assert!(
            own.base <= ip && ip < own.end,
            "Rip {ip:#x} outside {own:?}"
        );

        let mut frames = Vec::new();
        unwind(&mut context, 64, |pc| frames.push(pc));
        assert!(
            frames.len() >= 3,
            "only {} frames: {frames:x?}",
            frames.len()
        );
        let in_own = frames
            .iter()
            .filter(|pc| own.base <= **pc && **pc < own.end)
            .count();
        assert!(in_own >= 2, "{frames:x?}");
        let outside = frames.iter().any(|pc| !(own.base <= *pc && *pc < own.end));
        assert!(outside, "the walk never left the test binary: {frames:x?}");
    }

    #[test]
    fn the_clock_reads_the_present() {
        let ours = now_unix_millis();
        let std_now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        assert!((ours - std_now).abs() < 5_000, "{ours} vs {std_now}");
    }

    #[test]
    fn access_kinds_are_decoded_from_the_first_parameter() {
        assert_eq!(Access::from_parameter(0), Access::Read);
        assert_eq!(Access::from_parameter(1), Access::Write);
        assert_eq!(Access::from_parameter(8), Access::Execute);
        assert_eq!(Access::from_parameter(3), Access::Unknown(3));
    }

    #[test]
    fn an_unterminated_path_falls_back_to_stderr_instead_of_reading_past_it() {
        let file = ReportFile::open_append(&[u16::from(b'x')]);
        assert!(!file.owned);
    }
}
