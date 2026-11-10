//! The Windows crash report, proved by dying.
//!
//! **A real child process per fault type** — the thing under test ends the process, so it
//! cannot run in the test process, and mocking the exception away would leave every part
//! that matters unexercised: the pre-computed path, the hand-off to WER, the dispatch on a
//! real fault. Each child re-executes this test binary into [`the_crash_probe_child`],
//! which is a no-op in every ordinary run.
//!
//! **Every death runs twice**, armed and unarmed, and the exit codes must agree. That is
//! the proof of "termination unchanged" (TDD 21.4, GEP-35): the unarmed child is the
//! control, so the expected code is measured rather than written down from memory. The
//! code each test also names is a second, independent pin — it fails if Windows itself
//! ever starts ending one of these deaths differently. One death is exempt from the
//! agreement, because Windows itself does not end it the same way every time: see
//! [`a_stack_overflow_on_a_thread_rust_did_not_start_is_reported`].
//!
//! **The exit code alone does not prove WER still gets the crash**, and that was
//! measured, not reasoned: a filter mutated to end the process itself
//! (`TerminateProcess` with the exception's own code, the "make sure it dies" shortcut)
//! kept every exit code and passed every other assertion here, while WER never heard of
//! the crash. Nothing inside the process can tell the two apart — an `atexit` handler and
//! a thread-local destructor were both tried, and both stay silent either way. The one
//! difference is whether WER saw it, so [`a_crash_still_reaches_windows_error_reporting`]
//! asks WER. (Returning `EXCEPTION_EXECUTE_HANDLER` instead of handing the exception on is
//! NOT a swallow on Windows 10 19045: MEASURED, WER still logs the crash.)
//!
//! **Run these from PowerShell or `cmd`, not Git Bash, to see WER.** The MSYS runtime sets
//! `SEM_NOGPFAULTERRORBOX`, every child inherits it, and Windows then skips WER for all of
//! them. The WER test notices — its unarmed control logs nothing either — and SKIPs.
//!
//! **No death can pass vacuously.** The child prints a marker on the line before it
//! faults, and each test requires it: a wrong test name runs nothing and exits 0, and a
//! child that died of something else never prints it.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use super::super::ring::Ring;
use super::install;
use crate::platform::win32::crash::provoke;

/// Which death the child should die, by name. Absent in every ordinary run.
const PROBE: &str = "SCRIBOBULATE_CRASH_PROBE";
/// Where the child's report goes.
const PROBE_REPORT: &str = "SCRIBOBULATE_CRASH_PROBE_REPORT";
/// `1` to arm the handler first, `0` for the control.
const PROBE_ARMED: &str = "SCRIBOBULATE_CRASH_PROBE_ARMED";
/// Printed on the line before the fault — see the module header.
const REACHED: &str = "scribobulate crash probe: about to die of ";
/// Printed by the handled-exceptions child, followed by the vectored-handler count.
const FIRST_CHANCE: &str = "scribobulate crash probe: first-chance exceptions seen: ";
/// The breadcrumb the child records, so a report's breadcrumbs are checkable.
const CRUMB: &str = "scribobulate::probe: the crash probe is about to fault";
/// The identity block the child arms with.
const HEADER: &str = "scribobulate 0.1.0 (crash probe, test)\npid: probe\n";
/// A child that has not died in this long is hung, not slow — WER's collection included.
const CHILD_DEADLINE: Duration = Duration::from_secs(120);

/// `STATUS_STACK_BUFFER_OVERRUN`: how `__fastfail` ends a process, which is how the CRT's
/// `abort()` ends one after its `SIGABRT` handler returns.
const FAST_FAIL: u32 = 0xC000_0409;

#[derive(Clone, Copy, Debug)]
enum Fault {
    AccessViolation,
    IllegalInstruction,
    Breakpoint,
    StackOverflow,
    RawThreadStackOverflow,
    RawThreadAccessViolation,
    AccessViolationInGlib,
    Abort,
    GlibFatalMessage,
    Handled,
}

impl Fault {
    const ALL: [Fault; 10] = [
        Fault::AccessViolation,
        Fault::IllegalInstruction,
        Fault::Breakpoint,
        Fault::StackOverflow,
        Fault::RawThreadStackOverflow,
        Fault::RawThreadAccessViolation,
        Fault::AccessViolationInGlib,
        Fault::Abort,
        Fault::GlibFatalMessage,
        Fault::Handled,
    ];

    fn name(self) -> &'static str {
        match self {
            Fault::AccessViolation => "access-violation",
            Fault::IllegalInstruction => "illegal-instruction",
            Fault::Breakpoint => "breakpoint",
            Fault::StackOverflow => "stack-overflow",
            Fault::RawThreadStackOverflow => "raw-thread-stack-overflow",
            Fault::RawThreadAccessViolation => "raw-thread-access-violation",
            Fault::AccessViolationInGlib => "access-violation-in-glib",
            Fault::Abort => "abort",
            Fault::GlibFatalMessage => "glib-fatal-message",
            Fault::Handled => "handled",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|fault| fault.name() == name)
    }

    /// Die. Returns only for [`Fault::Handled`].
    fn provoke(self) {
        match self {
            Fault::AccessViolation => write_to_address_eight(),
            // SAFETY: deliberately fatal; nothing runs after either instruction.
            Fault::IllegalInstruction => unsafe { std::arch::asm!("ud2") },
            Fault::Breakpoint => unsafe { std::arch::asm!("int3") },
            Fault::StackOverflow => {
                recurse(0);
            }
            Fault::RawThreadStackOverflow => provoke::on_a_raw_thread(|| {
                recurse(0);
            }),
            Fault::RawThreadAccessViolation => provoke::on_a_raw_thread(write_to_address_eight),
            // `g_utf8_strlen` walks its argument to the NUL, so a pointer to address 1
            // faults INSIDE glib-2.0-0.dll: a third-party module, not ours.
            // SAFETY: deliberately fatal.
            Fault::AccessViolationInGlib => unsafe {
                glib::ffi::g_utf8_strlen(std::hint::black_box(1usize) as *const _, -1);
            },
            // SAFETY: deliberately fatal.
            Fault::Abort => unsafe { libc::abort() },
            // How GTK dies of `g_error`: a real G_LOG_LEVEL_ERROR through GLib itself, not
            // a stand-in for what we believe it does.
            // SAFETY: a NUL-terminated format and argument; does not return.
            Fault::GlibFatalMessage => unsafe {
                glib::ffi::g_log(
                    std::ptr::null(),
                    glib::ffi::G_LOG_LEVEL_ERROR,
                    c"%s".as_ptr(),
                    c"scribobulate crash probe: a GLib fatal message".as_ptr(),
                );
            },
            Fault::Handled => {
                let seen = provoke::handled_exceptions();
                eprintln!("{FIRST_CHANCE}{seen}");
            }
        }
    }
}

fn write_to_address_eight() {
    // SAFETY: deliberately fatal.
    unsafe { std::ptr::write_volatile(std::hint::black_box(8usize) as *mut u32, 1) };
}

#[inline(never)]
#[allow(unconditional_recursion)] // The stack overflow is the point.
fn recurse(depth: u64) -> u64 {
    let frame = std::hint::black_box([depth; 64]);
    recurse(depth + 1).wrapping_add(frame[7])
}

/// The doomed half: arm (or not), say so, die.
///
/// No restore of anything: this process is about to be killed by what it installs, and
/// there is no later test in it to protect (GEP-54's rule is about a surviving process).
#[test]
fn the_crash_probe_child() {
    let Ok(name) = std::env::var(PROBE) else {
        return;
    };
    let fault = Fault::parse(&name).expect("an unknown crash-probe fault name");
    if std::env::var(PROBE_ARMED).as_deref() == Ok("1") {
        let path = PathBuf::from(std::env::var(PROBE_REPORT).expect("no report path"));
        let ring: &'static Ring = Box::leak(Box::new(Ring::new()));
        ring.record(&format!("2026-11-04T12:00:00.000Z INFO  {CRUMB}"));
        install(&path, HEADER, ring);
    }
    eprintln!("{REACHED}{name}");
    fault.provoke();
    if !matches!(fault, Fault::Handled) {
        unreachable!("{name} returned instead of killing the process");
    }
}

struct Death {
    pid: u32,
    code: u32,
    stderr: String,
    report: Option<String>,
}

/// Run the child for `fault`, armed or not, with its report going to `report`.
fn die(fault: Fault, armed: bool, report: &Path) -> Death {
    let child_test = format!(
        "{}::the_crash_probe_child",
        module_path!()
            .strip_prefix(concat!(env!("CARGO_CRATE_NAME"), "::"))
            .unwrap_or(module_path!())
    );
    let mut child = Command::new(std::env::current_exe().expect("the test binary's own path"))
        .args(["--exact", &child_test, "--nocapture", "--test-threads=1"])
        .env(PROBE, fault.name())
        .env(PROBE_ARMED, if armed { "1" } else { "0" })
        .env(PROBE_REPORT, report)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("re-executing the test binary");
    // Read stderr on its own thread: a child blocked on a full pipe would read as a hang.
    let mut pipe = child.stderr.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = std::io::Read::read_to_string(&mut pipe, &mut text);
        text
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("waiting for the child") {
            break status;
        }
        if started.elapsed() > CHILD_DEADLINE {
            let _ = child.kill();
            panic!("the {} child hung for {CHILD_DEADLINE:?}", fault.name());
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let stderr = reader.join().unwrap_or_default();
    Death {
        pid: child.id(),
        // An NTSTATUS exit code arrives as a negative i32.
        code: status
            .code()
            .expect("a Windows process always has an exit code") as u32,
        report: std::fs::read_to_string(report).ok(),
        stderr,
    }
}

/// What a test expects of one death.
struct Expect {
    code: u32,
    /// A second code Windows itself may end this death with, in either child on its own.
    /// When set, each child must die of one of the two and the two need not agree.
    also: Option<u32>,
    /// Lines the report must contain, beyond the ones every report carries.
    lines: &'static [&'static str],
    /// A module the instruction pointer, or for `abort()` some backtrace frame, falls in;
    /// `None` means the test binary itself.
    module: Option<&'static str>,
}

/// Kill a child armed and a child unarmed, and hold the report against `expect`.
fn assert_a_reported_death(fault: Fault, expect: Expect) -> String {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    crate::platform::win32::create_private_directory(&state).expect("a private state dir");

    let control = die(fault, false, &state.join("crash-control.log"));
    let armed_path = state.join("crash-armed.log");
    let armed = die(fault, true, &armed_path);
    let name = fault.name();

    for (which, death) in [("control", &control), ("armed", &armed)] {
        assert!(
            death.stderr.contains(&format!("{REACHED}{name}")),
            "the {which} {name} child never reached its fault, so it proves nothing \
             (exit {:#010x}):\n{}",
            death.code,
            death.stderr
        );
    }
    if let Some(also) = expect.also {
        for (which, death) in [("control", &control), ("armed", &armed)] {
            assert!(
                death.code == expect.code || death.code == also,
                "{name}: the {which} child exited {:#010x}; Windows ends this death with \
                 {:#010x} or {also:#010x}",
                death.code,
                expect.code
            );
        }
    } else {
        assert_eq!(
            armed.code, control.code,
            "{name}: arming the handler changed how the process dies — armed exited \
             {:#010x}, unarmed {:#010x}",
            armed.code, control.code
        );
        assert_eq!(
            control.code, expect.code,
            "{name}: Windows ends this death differently than this test was written against"
        );
    }
    assert!(control.report.is_none(), "an unarmed child wrote a report");

    let text = armed.report.unwrap_or_else(|| {
        panic!(
            "{name}: the armed child died and wrote no report:\n{}",
            armed.stderr
        )
    });

    let mut required = vec![
        "=== scribobulate crash report ===",
        "crashed: ",
        "scribobulate 0.1.0 (crash probe, test)",
        "thread: ",
        CRUMB,
        "--- executable mappings ---",
        "--- backtrace ---",
    ];
    required.extend_from_slice(expect.lines);
    for line in required {
        assert!(text.contains(line), "{name}: missing {line:?} in:\n{text}");
    }

    // TDD 21.6: identity and fault before breadcrumbs before the map before the frames.
    let at = |needle: &str| text.find(needle).unwrap();
    let order = [
        at("scribobulate 0.1.0"),
        at(expect.lines[0]),
        at(CRUMB),
        at("--- executable mappings ---"),
        at("--- backtrace ---"),
    ];
    assert!(
        order.windows(2).all(|pair| pair[0] < pair[1]),
        "{name}: out of order:\n{text}"
    );

    // TDD 21.4/21.8: the fault resolves to a module + offset, and that module is in the map.
    let exe = std::env::current_exe().unwrap();
    let own = exe
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_ascii_lowercase();
    let module = expect.module.map_or(own.clone(), str::to_owned);
    let lower = text.to_ascii_lowercase();
    let located = format!("({module}+0x");
    assert!(
        lower.contains(&located),
        "{name}: nothing resolves to {located:?}:\n{text}"
    );
    let map = &lower[lower.find("--- executable mappings ---").unwrap()..];
    assert!(
        map.lines()
            .any(|l| l.starts_with("0x") && l.ends_with(&format!("\\{module}"))),
        "{name}: {module} has no load address in the module map:\n{text}"
    );
    assert!(
        map.lines()
            .any(|l| l.starts_with("0x") && l.ends_with(&format!("\\{own}"))),
        "{name}: the test binary has no load address in the module map:\n{text}"
    );

    // TDD 21.12: the report is owner-only. Windows has no mode bits, so the containing
    // directory carries it, and the report must hold exactly what it inherited from there.
    if let Err(why) = crate::platform::win32::inherits_only_from(&armed_path, &state) {
        panic!("{name}: the report is not owner-only: {why}");
    }
    text
}

#[test]
fn an_access_violation_is_reported_and_still_kills_the_process() {
    assert_a_reported_death(
        Fault::AccessViolation,
        Expect {
            code: 0xC000_0005,
            also: None,
            lines: &[
                "exception: EXCEPTION_ACCESS_VIOLATION (0xc0000005)",
                "kind: exception",
                "access: write",
                "fault address: 0x8\n",
            ],
            module: None,
        },
    );
}

#[test]
fn an_illegal_instruction_is_reported_and_still_kills_the_process() {
    assert_a_reported_death(
        Fault::IllegalInstruction,
        Expect {
            code: 0xC000_001D,
            also: None,
            lines: &["exception: EXCEPTION_ILLEGAL_INSTRUCTION (0xc000001d)"],
            module: None,
        },
    );
}

#[test]
fn a_breakpoint_is_reported_and_still_kills_the_process() {
    assert_a_reported_death(
        Fault::Breakpoint,
        Expect {
            code: 0x8000_0003,
            also: None,
            lines: &["exception: EXCEPTION_BREAKPOINT (0x80000003)"],
            module: None,
        },
    );
}

/// On a thread Rust started, which std gives a 20 KiB `SetThreadStackGuarantee`.
#[test]
fn a_stack_overflow_is_reported_and_still_kills_the_process() {
    assert_a_reported_death(
        Fault::StackOverflow,
        Expect {
            code: 0xC000_00FD,
            also: None,
            lines: &["exception: EXCEPTION_STACK_OVERFLOW (0xc00000fd)"],
            module: None,
        },
    );
}

/// The case the report's stack budget is set by: a thread with no guarantee beyond the
/// system's own, as GLib's workers are. MEASURED ~11.7 KiB free when the filter runs.
///
/// **Either child may die of an access violation instead**, so the two codes are not
/// compared. Once the stack has overflowed, Windows' own dispatch and unhandled-exception
/// path run in what little stack the thread has left; a second fault there ends the
/// process with `0xc0000005`. Seen once on CI, in the UNARMED control. MEASURED on
/// 19045 (debug build): that path has ~5.5 KiB to spare, and taking ~6 KiB of it flips
/// the death to `0xc0000005` whether WER runs or not. `recurse`'s frames are ~1.1 KiB,
/// so the recursion cannot skip the guard page; touching every page would change nothing.
/// Inferred, not measured: our filter has returned before Windows' path goes on, so it
/// cannot take that stack. The report checks carry this test instead: a filter that ran
/// out of stack mid-report leaves an incomplete report and fails them.
#[test]
fn a_stack_overflow_on_a_thread_rust_did_not_start_is_reported() {
    assert_a_reported_death(
        Fault::RawThreadStackOverflow,
        Expect {
            code: 0xC000_00FD,
            also: Some(0xC000_0005),
            lines: &["exception: EXCEPTION_STACK_OVERFLOW (0xc00000fd)"],
            module: None,
        },
    );
}

#[test]
fn a_fault_on_a_thread_rust_did_not_start_is_reported() {
    let text = assert_a_reported_death(
        Fault::RawThreadAccessViolation,
        Expect {
            code: 0xC000_0005,
            also: None,
            lines: &["exception: EXCEPTION_ACCESS_VIOLATION (0xc0000005)"],
            module: None,
        },
    );
    // The backtrace climbs out of the probe into the thread's start, which is kernel32's
    // `BaseThreadInitThunk` on every thread Windows starts.
    assert!(
        text.to_ascii_lowercase().contains("(kernel32.dll+0x"),
        "{text}"
    );
}

/// The case that started this: a fault inside a third-party DLL, not our code.
#[test]
fn a_fault_inside_a_gtk_stack_dll_is_reported_against_that_dll() {
    assert_a_reported_death(
        Fault::AccessViolationInGlib,
        Expect {
            code: 0xC000_0005,
            also: None,
            lines: &[
                "exception: EXCEPTION_ACCESS_VIOLATION (0xc0000005)",
                "access: read",
                "fault address: 0x1\n",
            ],
            module: Some("glib-2.0-0.dll"),
        },
    );
}

#[test]
fn abort_is_reported_and_still_fails_fast() {
    assert_a_reported_death(
        Fault::Abort,
        Expect {
            code: FAST_FAIL,
            also: None,
            lines: &["signal: SIGABRT (22), from abort()", "kind: signal"],
            module: Some("ucrtbase.dll"),
        },
    );
}

/// How GTK actually dies of `g_error` on Windows: GLib 2.88.1's `g_abort()` breakpoints
/// only under a debugger, and otherwise calls `abort()`. MEASURED here through GLib
/// itself, so a GLib that changes this fails the test rather than silently moving the
/// death somewhere the handler does not look.
#[test]
fn a_glib_fatal_message_is_reported_and_still_fails_fast() {
    let text = assert_a_reported_death(
        Fault::GlibFatalMessage,
        Expect {
            code: FAST_FAIL,
            also: None,
            lines: &["signal: SIGABRT (22), from abort()"],
            module: Some("glib-2.0-0.dll"),
        },
    );
    assert!(!text.contains("EXCEPTION_BREAKPOINT"), "{text}");
}

/// GEP-35 on Windows — report, never swallow: WER must still receive the crash.
///
/// Asked of WER itself, through its `Application Error` event (ID 1000), whose text names
/// the faulting process id. The unarmed child is the positive control: if WER logged
/// nothing for it either, this machine does not log crashes at all (or the run inherited
/// Git Bash's error mode), and the test says so rather than passing. Mutation guard, run:
/// end the process from the filter with `TerminateProcess` and the exception's own code,
/// and this fails while every exit-code assertion still passes (see the module header).
#[test]
fn a_crash_still_reaches_windows_error_reporting() {
    const WER_DEADLINE: Duration = Duration::from_secs(60);
    let dir = tempfile::tempdir().unwrap();
    let control = die(
        Fault::AccessViolation,
        false,
        &dir.path().join("control.log"),
    );
    let armed = die(Fault::AccessViolation, true, &dir.path().join("armed.log"));
    assert!(
        armed.report.is_some(),
        "the armed child wrote no report:\n{}",
        armed.stderr
    );

    // WER logs asynchronously, so poll.
    let started = Instant::now();
    let (control_seen, armed_seen) = loop {
        let pids = faulting_pids(&application_error_events());
        let seen = (pids.contains(&control.pid), pids.contains(&armed.pid));
        let (control_seen, armed_seen) = seen;
        if (control_seen && armed_seen) || started.elapsed() > WER_DEADLINE {
            break seen;
        }
        std::thread::sleep(Duration::from_secs(1));
    };
    if !control_seen {
        crate::testsymlink::skipped(
            "GEP-35 WER still receives a crash",
            "WER logged no Application Error event even for the unarmed control child, so \
             this machine does not record crashes and nothing here can be compared",
        );
        return;
    }
    assert!(
        armed_seen,
        "WER logged the unarmed child's crash (pid {:#x}) but not the armed one's (pid {:#x}): \
         the filter took the crash away from Windows Error Reporting",
        control.pid, armed.pid
    );
}

/// Recent `Application Error` events, lower-cased, through PowerShell — the event log has
/// no std or shell-free route, and this is a test.
fn application_error_events() -> String {
    let script = "Get-WinEvent -FilterHashtable @{LogName='Application'; \
                  ProviderName='Application Error'; Id=1000; \
                  StartTime=(Get-Date).AddMinutes(-10)} -ErrorAction SilentlyContinue | \
                  ForEach-Object { $_.Message }";
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .expect("running powershell");
    String::from_utf8_lossy(&output.stdout).to_ascii_lowercase()
}

/// Every pid an `Application Error` text names, from its `Faulting process id: 0x…` line.
fn faulting_pids(events: &str) -> Vec<u32> {
    const LABEL: &str = "faulting process id: 0x";
    events
        .match_indices(LABEL)
        .filter_map(|(at, _)| {
            let digits: String = events[at + LABEL.len()..]
                .chars()
                .take_while(char::is_ascii_hexdigit)
                .collect();
            u32::from_str_radix(&digits, 16).ok()
        })
        .collect()
}

#[test]
fn a_faulting_pid_is_read_whole_not_by_prefix() {
    let text = "faulting process id: 0x2380\r\nfaulting process id: 0x1a\n";
    assert_eq!(faulting_pids(text), [0x2380, 0x1a]);
    assert!(!faulting_pids(text).contains(&0x23));
}

/// QA round 5, H-2, on Windows: the crash writer appends to a report the panic hook
/// already wrote at the same path, rather than replacing the better evidence. Mutation
/// guard: open the report with `CREATE_ALWAYS` and the panic's text is gone.
#[test]
fn a_crash_does_not_destroy_the_panic_reports_evidence() {
    const PANIC_REPORT: &str = "=== scribobulate crash report ===\n\nkind: panic\npanic: the \
                                hook wrote this first\n";
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    crate::platform::win32::create_private_directory(&state).expect("a private state dir");
    let path = state.join("crash-shared.log");
    std::fs::write(&path, PANIC_REPORT).unwrap();

    let death = die(Fault::AccessViolation, true, &path);
    assert_eq!(death.code, 0xC000_0005, "{}", death.stderr);
    let text = death.report.expect("the report vanished");
    assert!(
        text.starts_with(PANIC_REPORT),
        "the crash writer destroyed the panic hook's report:\n{text}"
    );
    assert!(
        text[PANIC_REPORT.len()..].contains("EXCEPTION_ACCESS_VIOLATION"),
        "the crash was not appended:\n{text}"
    );
}

/// A first-chance exception that something handles writes NO report — the reason this
/// takes the unhandled filter rather than a vectored handler. The child's own vectored
/// count is the positive control: it proves the exceptions were raised.
#[test]
fn a_handled_exception_writes_no_report() {
    let dir = tempfile::tempdir().unwrap();
    let report = dir.path().join("crash-handled.log");
    let death = die(Fault::Handled, true, &report);
    assert!(
        death.stderr.contains(&format!("{REACHED}handled")),
        "{}",
        death.stderr
    );
    let seen: usize = death
        .stderr
        .lines()
        .find_map(|l| l.strip_prefix(FIRST_CHANCE))
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or_else(|| panic!("no first-chance count:\n{}", death.stderr));
    assert!(
        seen >= 3,
        "only {seen} first-chance exceptions were raised:\n{}",
        death.stderr
    );
    assert_eq!(
        death.code, 0,
        "the handled-exception child died:\n{}",
        death.stderr
    );
    assert!(
        death.report.is_none(),
        "a handled exception wrote a crash report:\n{}",
        death.report.unwrap_or_default()
    );
}

/// The handler's reach is the set of modules it may touch, and every one must be safe to
/// run with the heap or loader lock held by whoever faulted. `signal.rs` holds the same
/// line for unix. Widening it is legitimate; widening it without the argument is the bug.
#[test]
fn the_handler_reaches_only_crash_safe_code() {
    let source = include_str!("../exception.rs");
    let paths = |prefix: &str| {
        let mut found: Vec<&str> = source
            .match_indices(prefix)
            .map(|(i, _)| {
                let rest = &source[i + prefix.len()..];
                let end = rest
                    .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .unwrap_or(rest.len());
                &rest[..end]
            })
            .filter(|ident| !ident.is_empty())
            .collect();
        found.sort_unstable();
        found.dedup();
        found
    };
    // `signal` is a doc link to the sibling module; `install` is mod.rs's installer named
    // in a doc link. The rest are `signal.rs`'s own allow-list.
    assert_eq!(
        paths("super::"),
        ["install", "rawbuf", "ring", "signal", "timefmt"],
        "the set of `super::` references from the crash handler changed"
    );
    assert_eq!(
        paths("crate::"),
        ["platform"],
        "the crash handler reaches into the crate beyond the Win32 crash seam"
    );
    assert!(
        source.contains("use crate::platform::win32::crash::{"),
        "the only `crate::` reach must be the crash seam"
    );
    for banned in [
        "format!",
        "String",
        "Vec<",
        "to_string",
        "Box::new",
        "Mutex",
        "log::",
    ] {
        let after_install = &source[source.find("fn on_crash").unwrap()..];
        assert!(
            !after_install.contains(banned),
            "`{banned}` appears on the crash path, which may not allocate or lock"
        );
    }
}
