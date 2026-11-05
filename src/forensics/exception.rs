//! The Windows crash report: what runs when the process is already dying.
//!
//! The Windows counterpart of [`super::signal`], for a platform with no POSIX signals.
//! A native crash on Windows is an unhandled SEH exception or a CRT `abort()`, and
//! `platform::win32::crash` is the seam that receives both; this module decides what
//! the report says. Before it, an access violation inside `gtk-4-1.dll` (`0xc0000005`,
//! seen on a user's machine) reached Windows Error Reporting and left nothing of ours.
//!
//! # The same report, and the same constraints
//!
//! The layout is `signal.rs`'s and `report::write_report`'s, so one reader reads all
//! three: banner, `kind:`, `crashed:`, the identity block, the fault, the breadcrumbs,
//! then the module map and the backtrace (TDD 21.4–21.8). `kind: exception` names an SEH
//! death and `kind: signal` an `abort()`, which the CRT delivers as `SIGABRT`.
//!
//! The path it runs on may neither allocate nor lock — the heap's or the loader's lock
//! may be held by the very code that faulted. So, as in `signal.rs`: the report path and
//! the identity block are prepared at install time and leaked, formatting is integers
//! into stack buffers ([`super::rawbuf`]), breadcrumbs come from the lock-free
//! [`super::ring`], and every OS call goes through the seam, whose header argues each one.
//!
//! # Two departures from the unix layout, both for the reader
//!
//! **The instruction pointer names its module inline** — `0x7ff8… (gtk-4-1.dll+0x1a2b3)`
//! — as does every backtrace frame. Unix leaves that subtraction to the reader against the
//! mappings; here the module lookup is cheap and lock-free, so the report does it.
//!
//! **The module map comes before the backtrace.** Unwinding is the one step that reads a
//! possibly corrupt stack, so it is the step most likely to fault. Written last, a fault
//! there costs only itself (TDD 21.6), and the map — which makes every address in the
//! report resolvable — is already on disk.
//!
//! # What it costs when nothing goes wrong
//!
//! One `SetUnhandledExceptionFilter`, one `signal`, and the leaked path and identity
//! block. No thread, no reserved stack, nothing on any path that runs while the
//! application is healthy (TDD 21.11).

use core::fmt::Write as _;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

use crate::platform::win32::crash::{
    self, Access, Cause, Crash, Module, ModulePath, ReportFile, EXCEPTION_ACCESS_VIOLATION,
    EXCEPTION_IN_PAGE_ERROR,
};

use super::rawbuf::RawBuf;
use super::ring::Ring;

/// Handler-readable state, published once at install time — atomics rather than a
/// `OnceLock`, for `signal.rs`'s reason.
static REPORT_PATH: AtomicPtr<u16> = AtomicPtr::new(std::ptr::null_mut());
static REPORT_PATH_LEN: AtomicUsize = AtomicUsize::new(0);
static HEADER_PTR: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());
static HEADER_LEN: AtomicUsize = AtomicUsize::new(0);
static BREADCRUMBS: AtomicPtr<Ring> = AtomicPtr::new(std::ptr::null_mut());

/// The deepest backtrace written. Matches `signal.rs`.
const MAX_FRAMES: usize = 64;

/// Arm the handler. Called once, from [`super::install`].
///
/// `report_path` is created only if the process dies, so an ordinary run leaves nothing.
pub(crate) fn install(report_path: &std::path::Path, header: &str, ring: &'static Ring) {
    use std::os::windows::ffi::OsStrExt as _;
    let wide: Vec<u16> = report_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    if wide[..wide.len() - 1].contains(&0) {
        // Unreachable for a path Windows can name, but a crash handler that panicked
        // while installing would be a self-inflicted outage.
        log::warn!("crash reports disabled: the report path contains a NUL");
        return;
    }
    // Leaked deliberately: they must outlive every crash this process can have.
    let wide: &'static [u16] = Box::leak(wide.into_boxed_slice());
    REPORT_PATH_LEN.store(wide.len(), Ordering::Release);
    REPORT_PATH.store(wide.as_ptr().cast_mut(), Ordering::Release);
    let header: &'static [u8] = Box::leak(header.as_bytes().to_vec().into_boxed_slice());
    HEADER_LEN.store(header.len(), Ordering::Release);
    HEADER_PTR.store(header.as_ptr().cast_mut(), Ordering::Release);
    BREADCRUMBS.store(std::ptr::from_ref(ring).cast_mut(), Ordering::Release);

    crash::install(on_crash);
}

/// The report, written on the dying thread. Everything reachable from here is
/// allocation- and lock-free; `the_handler_reaches_only_crash_safe_code` holds the set.
fn on_crash(crash: &Crash) {
    let file = ReportFile::open_append(report_path());

    file.write(b"=== scribobulate crash report ===\n\nkind: ");
    file.write(match crash.cause {
        Cause::Exception { .. } => b"exception",
        Cause::Abort => b"signal",
    });
    file.write(b"\ncrashed: ");
    let mut stamp: RawBuf<{ super::timefmt::ISO8601_LEN + 8 }> = RawBuf::new();
    let _ =
        super::timefmt::civil_from_unix_millis(crash::now_unix_millis()).write_iso8601(&mut stamp);
    file.write(stamp.as_bytes());
    file.write(b"\n");

    // The identity block, verbatim — rendered before anything went wrong.
    let header = HEADER_PTR.load(Ordering::Acquire);
    if !header.is_null() {
        // SAFETY: leaked at install time, never freed, length stored beside it.
        file.write(unsafe {
            std::slice::from_raw_parts(header, HEADER_LEN.load(Ordering::Acquire))
        });
    }

    write_fault(&file, crash);
    write_breadcrumbs(&file);
    write_module_map(&file);
    write_backtrace(&file, crash);
}

fn report_path() -> &'static [u16] {
    let path = REPORT_PATH.load(Ordering::Acquire);
    if path.is_null() {
        return &[];
    }
    // SAFETY: leaked at install time, never freed, length stored before the pointer.
    unsafe { std::slice::from_raw_parts(path, REPORT_PATH_LEN.load(Ordering::Acquire)) }
}

/// The fault block: what died, where, and from which instruction (TDD 21.4).
fn write_fault(file: &ReportFile, crash: &Crash) {
    let mut buf: RawBuf<192> = RawBuf::new();
    match crash.cause {
        Cause::Exception {
            code,
            fault_address,
            access,
            instruction_pointer,
        } => {
            let _ = writeln!(buf, "\nexception: {} ({code:#010x})", exception_name(code));
            if let Some(access) = access {
                let _ = writeln!(buf, "access: {}", access_name(access));
            }
            let _ = writeln!(buf, "fault address: {fault_address:#x}");
            let _ = write!(buf, "instruction pointer: {instruction_pointer:#x} ");
            file.write(buf.as_bytes());
            write_location(file, instruction_pointer);
            file.write(b"\n");
        }
        Cause::Abort => {
            let _ = writeln!(buf, "\nsignal: SIGABRT ({}), from abort()", libc::SIGABRT);
            let _ = writeln!(buf, "fault address: (none: abort() is a call, not a fault)");
            let _ = writeln!(
                buf,
                "instruction pointer: (none: the backtrace runs through abort() to its caller)"
            );
            file.write(buf.as_bytes());
        }
    }
    let mut thread: RawBuf<32> = RawBuf::new();
    let _ = writeln!(thread, "thread: {}", crash.thread_id);
    file.write(thread.as_bytes());
}

/// `(module+0xoffset)` for an address inside a loaded image, else `(in no module)`.
fn write_location(file: &ReportFile, address: usize) {
    match crash::module_containing(address) {
        Some(module) => {
            file.write(b"(");
            write_wide(file, ModulePath::of(module).file_name());
            let mut offset: RawBuf<24> = RawBuf::new();
            let _ = write!(offset, "+{:#x})", address - module.base);
            file.write(offset.as_bytes());
        }
        None => file.write(b"(in no module)"),
    }
}

/// UTF-16 to UTF-8 onto the file, through a small stack buffer — a module path is the
/// one unbounded text here.
fn write_wide(file: &ReportFile, units: &[u16]) {
    const CHUNK: usize = 128;
    /// The most bytes one `char` encodes to.
    const UTF8_MAX: usize = 4;
    let mut buf: RawBuf<CHUNK> = RawBuf::new();
    for c in char::decode_utf16(units.iter().copied()) {
        if buf.as_bytes().len() + UTF8_MAX > CHUNK {
            file.write(buf.as_bytes());
            buf = RawBuf::new();
        }
        let _ = buf.write_char(c.unwrap_or(char::REPLACEMENT_CHARACTER));
    }
    file.write(buf.as_bytes());
}

/// The breadcrumbs — what the application was doing (TDD 21.5).
fn write_breadcrumbs(file: &ReportFile) {
    file.write(b"\n--- breadcrumbs, oldest first ---\n");
    let ring = BREADCRUMBS.load(Ordering::Acquire);
    if ring.is_null() {
        file.write(b"(unavailable)\n");
        return;
    }
    // SAFETY: a `&'static Ring` published at install time and never freed.
    let ring = unsafe { &*ring };
    let mut any = false;
    ring.for_each(|line| {
        any = true;
        file.write(line);
        file.write(b"\n");
    });
    if !any {
        file.write(b"(none recorded)\n");
    }
}

/// Every loaded image with its load address, so any address in the report resolves to
/// module + offset by subtraction (TDD 21.8) — the counterpart of `/proc/self/maps`.
fn write_module_map(file: &ReportFile) {
    file.write(b"\n--- executable mappings ---\n");
    crash::for_each_module(|module: Module| {
        let mut range: RawBuf<48> = RawBuf::new();
        let _ = write!(range, "{:#018x}-{:#018x} ", module.base, module.end);
        file.write(range.as_bytes());
        write_wide(file, ModulePath::of(module).units());
        file.write(b"\n");
    });
}

/// Frames, last: the shipped binary is stripped, so they resolve as module + offset,
/// which each line carries.
fn write_backtrace(file: &ReportFile, crash: &Crash) {
    file.write(b"\n--- backtrace ---\n");
    if !crash::CAN_UNWIND {
        file.write(b"(unavailable on this architecture)\n");
        return;
    }
    let mut index = 0usize;
    crash.for_each_frame(MAX_FRAMES, |pc| {
        let mut frame: RawBuf<40> = RawBuf::new();
        let _ = write!(frame, "#{index:<2} {pc:#018x} ");
        file.write(frame.as_bytes());
        write_location(file, pc);
        file.write(b"\n");
        index += 1;
    });
}

fn access_name(access: Access) -> &'static str {
    match access {
        Access::Read => "read",
        Access::Write => "write",
        Access::Execute => "execute (data-execution prevention)",
        Access::Unknown(_) => "unknown",
    }
}

/// The `winnt.h` / `ntstatus.h` name of an exception code. An unnamed code still prints
/// its number, which is what a lookup needs.
fn exception_name(code: u32) -> &'static str {
    match code {
        EXCEPTION_ACCESS_VIOLATION => "EXCEPTION_ACCESS_VIOLATION",
        EXCEPTION_IN_PAGE_ERROR => "EXCEPTION_IN_PAGE_ERROR",
        0xC000_001D => "EXCEPTION_ILLEGAL_INSTRUCTION",
        0xC000_0096 => "EXCEPTION_PRIV_INSTRUCTION",
        0x8000_0003 => "EXCEPTION_BREAKPOINT",
        0x8000_0004 => "EXCEPTION_SINGLE_STEP",
        0xC000_00FD => "EXCEPTION_STACK_OVERFLOW",
        0xC000_0094 => "EXCEPTION_INT_DIVIDE_BY_ZERO",
        0xC000_0095 => "EXCEPTION_INT_OVERFLOW",
        0xC000_008C => "EXCEPTION_ARRAY_BOUNDS_EXCEEDED",
        0x8000_0002 => "EXCEPTION_DATATYPE_MISALIGNMENT",
        0xC000_008D..=0xC000_0093 => "EXCEPTION_FLT_*",
        0xC000_0025 => "EXCEPTION_NONCONTINUABLE_EXCEPTION",
        0xC000_0026 => "EXCEPTION_INVALID_DISPOSITION",
        0xC000_0374 => "STATUS_HEAP_CORRUPTION",
        0xE06D_7363 => "MSVC C++ exception",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests;
