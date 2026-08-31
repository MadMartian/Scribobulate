//! The main-thread GTK test suite: a second crate root, run as a `harness = false`
//! target so that `main()` — and therefore every test body — executes on the process
//! main thread.
//!
//! # Why a second crate root rather than the library
//!
//! The bodies live in `#[cfg(all(test, feature = "gtk-integration-tests"))]` modules
//! throughout `src/`. An ordinary `tests/*.rs` target links the **non-test** library,
//! where those modules do not exist, and it can only see `pub` items in any case (the
//! tree is `pub(crate)` throughout, and `pub use` cannot widen that — E0364). A root
//! placed *here*, beside `lib.rs`, is compiled `--cfg test` from the same sources, so
//! it sees every gated module, every private item, and the dev-dependencies they use,
//! with no visibility changes and no `#[path]` anywhere: `mod copymap;` resolves to
//! `src/copymap.rs` and its children to `src/copymap/`, exactly as `lib.rs` resolves
//! them — placing a second crate root anywhere *other* than beside `lib.rs` reopens
//! `#[path]`'s child-resolution trap (ScrAP-197).
//!
//! # Why the crate-level `allow`
//!
//! Cargo builds this target with `--cfg test` but **not** `--test`, and rustc removes
//! `#[test]` items when `--test` is absent. The 27 plain `#[test]` functions inside
//! those gated modules therefore vanish here, orphaning any helper only they call
//! into `never used` — an error under the `-D warnings` gate. Silencing that is
//! correct in *this* file, which is a test root and ships nothing; it would not be
//! correct on the library, which is the whole reason the runner is not hosted there.
//! Nothing is lost: the same code is linted normally in the lib and lib-test targets.
//!
//! # Run
//!
//! `xvfb-run` OUTSIDE, `dbus-run-session` INSIDE — the reverse order leaks the bus's
//! activated daemons (portal, gvfs, a11y) onto the developer's REAL X server, where they
//! outlive the run and accumulate until Xorg refuses new clients. POLICY step 5 carries
//! the measurement; `scripts/run-integration.sh` is the whole step and does this for you.
//!
//! ```sh
//! xvfb-run -a dbus-run-session -- cargo test --features gtk-integration-tests --test gtk_suite
//! xvfb-run -a dbus-run-session -- cargo test --features gtk-integration-tests --test gtk_suite -- --list
//! xvfb-run -a dbus-run-session -- cargo test --features gtk-integration-tests --test gtk_suite -- copymap
//! ```
#![allow(dead_code, unused_imports, unused_macros)]

// ── The module tree ────────────────────────────────────────────────────────────
//
// A verbatim copy of `lib.rs`'s list, and the one hand-maintained thing about this
// shape. Drift here is SILENT — a new top-level module added to `lib.rs` and not to
// this list drops every body in it from the suite, with nothing failing — so
// `cargo xtask lint-references` check 4 compares the two lists as a build gate. Do not
// "tidy" the duplication away without replacing that gate.
mod a11y;
mod accel;
mod affordance;
mod annotate;
mod annotations;
mod annotations_view;
mod app;
mod atomic_io;
mod clipboard;
mod codeview;
#[cfg(any(windows, target_os = "macos"))]
mod colorscheme;
mod config;
mod copymap;
mod cssfrag;
mod decorplan;
mod docio;
mod export;
mod farscroll;
mod forensics;
mod format;
mod icons;
mod imagefetch;
mod keynav;
mod limits;
mod lineendings;
mod links;
mod logging;
mod macwordnav;
mod mdtable;
mod outline;
mod outline_view;
mod palette;
mod pangospan;
mod platform;
mod preview;
mod readingpos;
mod renderer;
mod saferizer;
mod session;
mod span;
mod suite_registry;
mod swapfile;
mod tags;
mod tasklist;
// Gated `#[cfg(all(test, feature = "gtk-integration-tests"))]` in `lib.rs`; this root
// is always built `--cfg test` under that same feature (it only exists to run
// gtk-integration-tests bodies), so it needs no gate here — but it does need the
// declaration, or a body reaching `testpump` drops out of this main-thread run with
// nothing failing (`cargo xtask lint-references` check 4).
mod testpump;
// Test-only in `lib.rs` (`#[cfg(test)]`); this root is always built `--cfg test`, so
// it needs no gate here — but it does need the declaration, or the suite build breaks
// the moment a symlink test in it reaches the shared helper.
mod sprite;
mod testlog;
mod testsymlink;
mod theme;
mod widgets;
mod window;
mod winstate;
#[cfg(unix)]
mod workaround;

/// Re-exported, not re-declared: this root's `#![allow(dead_code)]` would have
/// masked a copy here drifting from `src/lib.rs`'s. See [`icons::APP_ID`].
///
/// That same `allow` would equally have masked a MISSING module declaration, which
/// is what the R1-06 move removed from this file — `win32` is now declared once in
/// `platform/mod.rs`, shared by both crate roots, instead of twice here and in
/// `lib.rs`. One mask, two silent-drift defects, closed from opposite ends.
pub(crate) use icons::APP_ID;

use suite_registry::Case;

/// Default per-case wall-clock budget. Generous: the slowest bodies present real
/// windows and pump the frame clock. Override with `-- --timeout <secs>`.
const DEFAULT_CASE_TIMEOUT_SECS: u32 = 120;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let list_only = args.iter().any(|a| a == "--list");
    let Cli {
        timeout,
        skips,
        filters,
    } = match parse_args(&args) {
        Ok(cli) => cli,
        Err(message) => {
            eprintln!("gtk_suite: {message}");
            eprintln!(
                "usage: gtk_suite [--list] [--skip <substring>]... [--timeout <secs>] [filter]..."
            );
            // 2, not 1: a usage error is not a test failure, and a caller that greps
            // the exit code should be able to tell "the suite ran and something
            // failed" from "the suite never started".
            std::process::exit(2);
        }
    };

    // Deterministic order. `inventory` iteration order follows link order, which is
    // not stable across builds — and a suite whose order moves between runs makes
    // any order-dependent failure unreproducible.
    let mut cases: Vec<&Case> = inventory::iter::<Case>.into_iter().collect();
    cases.sort_by_key(|c| c.name);

    let selected: Vec<&Case> = cases
        .iter()
        .copied()
        .filter(|c| filters.is_empty() || filters.iter().any(|f| c.name.contains(f)))
        .filter(|c| !skips.iter().any(|s| c.name.contains(s)))
        .collect();

    if list_only {
        for case in &selected {
            println!("{}", case.name);
        }
        println!("\n{} cases", selected.len());
        return;
    }

    // NOT `logging::init()`. That installs the glib→`log` bridge, which reformats
    // GLib's own output — `Gtk-WARNING **:` becomes `(Gtk) Warning:` — and both
    // tests and `tests/MANUAL-TEST.md` grep the documented token. A previous
    // attempt at this suite changed that format and broke them; the runner
    // deliberately leaves GLib's default handler in place.
    gtk::init().expect(
        "GTK init on the process main thread — this is the whole point of this target; \
         if it fails here, check that a display is available (xvfb-run) rather than \
         suspecting the harness",
    );

    println!(
        "running {} of {} cases on the process main thread (per-case timeout {}s)\n",
        selected.len(),
        cases.len(),
        timeout
    );

    let started = std::time::Instant::now();
    let mut failed: Vec<&str> = Vec::new();
    let mut ignored = 0usize;

    for case in &selected {
        print!("test {} ... ", case.name);
        flush();
        // `#[ignore]` means the same thing here as it does under libtest. Reported
        // per case rather than filtered out of `selected` earlier, so the count in
        // the summary still adds up and an ignored body is visible rather than
        // simply absent — a quarantined test that vanishes from the output is how a
        // quarantine becomes permanent.
        if case.ignored {
            println!("ignored");
            ignored += 1;
            flush();
            continue;
        }
        arm_timeout(case.name, timeout);
        // Each body is isolated to the extent a shared process allows: a panic is
        // caught and reported, and the run continues. It is NOT isolated from
        // process-global GTK state — see the note on that in the plan; a body that
        // needs a pristine display, icon theme or GtkSettings belongs in its own
        // `harness = false` target, not here.
        let outcome = std::panic::catch_unwind(case.run);
        disarm_timeout();
        // `#[should_panic]` inverts the verdict, exactly as libtest inverts it: a
        // caught panic is the expected outcome (PASS) and a clean return is the
        // failure. Otherwise a `#[should_panic]` body that panics as documented has
        // its panic caught and reported FAILED, the opposite of what the author wrote.
        match (outcome, case.should_panic) {
            (Ok(()), false) => println!("ok"),
            (Err(_), true) => println!("ok"),
            (Ok(()), true) => {
                println!("FAILED");
                eprintln!(
                    "    note: test {} was expected to panic, but it did not",
                    case.name
                );
                failed.push(case.name);
            }
            (Err(_), false) => {
                println!("FAILED");
                failed.push(case.name);
            }
        }
        flush();
    }

    println!(
        "\nresult: {}. {} passed; {} failed; {} ignored; finished in {:.2}s",
        if failed.is_empty() { "ok" } else { "FAILED" },
        selected.len() - failed.len() - ignored,
        failed.len(),
        ignored,
        started.elapsed().as_secs_f64()
    );
    if !failed.is_empty() {
        println!("\nfailures:");
        for name in &failed {
            println!("    {name}");
        }
        std::process::exit(1);
    }
}

/// What this runner understands on the command line.
struct Cli<'a> {
    timeout: u32,
    /// libtest's `--skip <substring>`, repeatable — cases matching any are omitted.
    skips: Vec<&'a str>,
    /// Positional substrings, libtest's filter behaviour, so muscle memory transfers.
    filters: Vec<&'a str>,
}

/// One pass, consuming each value-taking flag's value explicitly.
///
/// The explicitness is the point. An unrecognised *flag* is harmless — it is dropped
/// by the `--` prefix test. An unrecognised flag's *value* is not: it is a bare word,
/// so it falls through to the filter list and turns an omission into a selection.
/// `--skip <name>` used to do exactly that here, inverting "run everything except this
/// one" into "run only this one", silently and with an exit code of 0 —
/// `packaging/windows/pipeline.ps1`'s step 5 ran 1 case of 149 that way. Hence a real
/// parse rather than a filter over the raw argv, and hence `--skip` is supported at
/// all: the caller who carves a known-failing case out by name is the one who most
/// needs this target to behave like libtest.
fn parse_args(args: &[String]) -> Result<Cli<'_>, String> {
    const VALUE_FLAGS: [&str; 2] = ["--skip", "--timeout"];
    let (mut timeout, mut skips, mut filters) = (DEFAULT_CASE_TIMEOUT_SECS, Vec::new(), Vec::new());
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) => (flag, Some(value)),
            None => (arg, None),
        };
        if VALUE_FLAGS.contains(&flag) {
            let value = inline.or_else(|| args.get(i + 1).map(String::as_str));
            match (flag, value) {
                ("--skip", Some(value)) => skips.push(value),
                ("--timeout", Some(value)) => {
                    // REJECTED, not defaulted. A mistyped budget used to be dropped
                    // in silence and the run continued at the 120s default — the same
                    // shape as the `--skip` defect this function exists to close: the
                    // caller's instruction is discarded and the suite still exits 0,
                    // so a CI job that meant `--timeout 5` waits 120 and calls it a
                    // pass. An argument that cannot be honoured must not be ignored.
                    timeout = value.parse::<u32>().map_err(|_| {
                        format!("--timeout expects a whole number of seconds, got `{value}`")
                    })?;
                }
                (flag, None) => {
                    // A value-taking flag at the end of argv. Silently ignoring it
                    // means `--skip` with a typo'd trailing name runs the whole suite
                    // including the case that was meant to be carved out.
                    return Err(format!("{flag} requires a value"));
                }
                (flag, Some(_)) => {
                    // Unreachable while VALUE_FLAGS holds exactly the two arms above —
                    // and REJECTING rather than ignoring is the point: adding a third
                    // entry to VALUE_FLAGS without an arm here would otherwise consume
                    // its value and discard the instruction, which is the whole defect
                    // this function was written to close.
                    return Err(format!("{flag} is declared in VALUE_FLAGS but unhandled"));
                }
            }
            if inline.is_none() {
                i += 1; // the value is consumed, never a filter
            }
        } else if !arg.starts_with("--") {
            filters.push(arg);
        }
        i += 1;
    }
    Ok(Cli {
        timeout,
        skips,
        filters,
    })
}

fn flush() {
    use std::io::Write;
    let _ = std::io::stdout().flush();
}

// ── Per-case wall-clock cap ────────────────────────────────────────────────────
//
// A harness-free target gets no libtest timeout, and this runner has no worker
// thread to abandon — a body that never returns would hang the suite forever, which
// on a CI runner means a job that is killed with no indication of which case did it.
//
// `alarm(2)` gives a real cap with no threads: the kernel raises SIGALRM even if the
// body is blocked inside GTK or spinning in a main-loop pump. The handler is
// async-signal-safe by construction — it does nothing but `write(2)` a pre-stored
// name and `_exit`.
//
// Deliberately NOT also capping output volume. The recorded case of a runaway suite
// (a non-terminating widget-dispose loop emitting gigabytes) is caught here, because
// the flood was a symptom of the hang rather than an independent failure: the cap
// that matters is the one on time. Intercepting a body's own stdout would need a
// pipe and a pump to drain it, and a body that out-writes an undrained pipe blocks —
// i.e. it would convert a noisy pass into a hang, trading a real failure mode for a
// worse one. Redirect to a file and set `ulimit -f` if a hard byte cap is ever
// wanted.
#[cfg(unix)]
static TIMED_OUT_CASE: std::sync::atomic::AtomicPtr<u8> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
#[cfg(unix)]
static TIMED_OUT_LEN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(unix)]
fn arm_timeout(name: &'static str, secs: u32) {
    use std::sync::atomic::Ordering;
    TIMED_OUT_CASE.store(name.as_ptr().cast_mut(), Ordering::SeqCst);
    TIMED_OUT_LEN.store(name.len(), Ordering::SeqCst);
    // SAFETY: installing a signal handler and arming the process alarm. The handler
    // below touches only atomics and the two async-signal-safe calls `write`/`_exit`.
    unsafe {
        // Via `*const ()`: casting a function item straight to an integer is a lint
        // (the item is not a pointer yet), and `sighandler_t` is an integer type.
        libc::signal(libc::SIGALRM, on_alarm as *const () as libc::sighandler_t);
        libc::alarm(secs);
    }
}

#[cfg(unix)]
fn disarm_timeout() {
    // SAFETY: cancels any pending alarm; no handler state is read.
    unsafe {
        libc::alarm(0);
    }
}

#[cfg(unix)]
extern "C" fn on_alarm(_sig: libc::c_int) {
    use std::sync::atomic::Ordering;
    const PREFIX: &str = "\nTIMED OUT (per-case wall-clock cap): ";
    const SUFFIX: &str = "\nthe suite aborts here: a body that does not return cannot be \
                          abandoned without a thread to abandon it on.\n";
    let name = TIMED_OUT_CASE.load(Ordering::SeqCst);
    let len = TIMED_OUT_LEN.load(Ordering::SeqCst);
    // SAFETY: async-signal-safe only. `write` and `_exit` are on the permitted list;
    // the pointer is a 'static str set before the alarm was armed.
    unsafe {
        libc::write(2, PREFIX.as_ptr().cast(), PREFIX.len());
        if !name.is_null() {
            libc::write(2, name.cast(), len);
        }
        libc::write(2, SUFFIX.as_ptr().cast(), SUFFIX.len());
        libc::_exit(124);
    }
}

/// No `alarm(2)` outside unix. The cap is a diagnostic aid, not a correctness
/// property, so the suite runs uncapped here rather than not running.
#[cfg(not(unix))]
fn arm_timeout(_name: &'static str, _secs: u32) {}
#[cfg(not(unix))]
fn disarm_timeout() {}

/// The runner's own argument parsing, guarded here because its failure mode is
/// silent and green: a `--skip <name>` whose value leaked into `filters` inverted
/// the request into "run only that case" and still exited 0, so
/// `packaging/windows/pipeline.ps1`'s step 5 reported success having run 1 case of
/// 149. Registered as a suite case (a plain `#[test]` would be stripped — this
/// target is built `--cfg test` but not `--test`, see the module docs).
#[gtktest::test]
fn parse_args_excludes_skipped_cases_instead_of_selecting_them() {
    let argv = |args: &[&str]| -> Vec<String> { args.iter().map(|a| a.to_string()).collect() };

    // The exact step-5 invocation from pipeline.ps1.
    let args = argv(&["--test-threads=1", "--skip", "a_known_failure"]);
    let cli = parse_args(&args).expect("the pipeline's own argv must parse");
    assert_eq!(cli.skips, ["a_known_failure"]);
    assert!(
        cli.filters.is_empty(),
        "the skipped NAME must never become a positive filter — that inverts the \
         carve-out into a selection: {:?}",
        cli.filters
    );

    // `--skip=NAME`, repeated, alongside a real filter and a value-taking flag whose
    // value is also a bare word.
    let args = argv(&["--skip=one", "--skip", "two", "--timeout", "30", "copymap"]);
    let cli = parse_args(&args).expect("a well-formed invocation must parse");
    assert_eq!(cli.skips, ["one", "two"]);
    assert_eq!(cli.filters, ["copymap"]);
    assert_eq!(cli.timeout, 30);

    // An unknown flag is ignored, but its value must not be read as a filter either.
    let args = argv(&["--nocapture", "copymap"]);
    let cli = parse_args(&args).expect("an unknown flag is not an error");
    assert_eq!(cli.filters, ["copymap"]);
    assert_eq!(cli.timeout, DEFAULT_CASE_TIMEOUT_SECS);

    // ── The half the original guard did not cover ────────────────────────────
    // Everything above asserts the `--skip` leak stays closed, which it already
    // was. These assert the OTHER way an argument can be silently discarded: a
    // value the runner cannot use. Both used to fall through to `_ => {}` and run
    // the suite anyway, at the default budget or with no exclusion at all.
    let args = argv(&["--timeout", "banana"]);
    assert!(
        parse_args(&args).is_err(),
        "a non-numeric --timeout must be rejected, not silently defaulted"
    );

    let args = argv(&["--timeout=0x10"]);
    assert!(
        parse_args(&args).is_err(),
        "the inline form must reject the same values as the spaced form"
    );

    let args = argv(&["copymap", "--skip"]);
    assert!(
        parse_args(&args).is_err(),
        "--skip with no value must be rejected: silently ignoring it runs the very \
         case the caller asked to carve out"
    );

    let args = argv(&["--timeout"]);
    assert!(
        parse_args(&args).is_err(),
        "--timeout with no value must be rejected for the same reason"
    );
}
