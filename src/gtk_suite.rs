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
//! `#[path]`'s child-resolution trap (GEP-34).
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
//! # One process per test module
//!
//! `main()` is two programs. Invoked normally it is the **driver**: it selects the
//! cases, never initialises GTK, groups them by the test module that declares them
//! (`a::b::gtk_integration_tests::case` → `a::b::gtk_integration_tests`), and runs
//! each group by re-executing this binary with one `--run-case <name>` per case. That
//! **child** initialises GTK once, runs its cases in order on its main thread, and
//! records each one's start and verdict in a report file the driver watches.
//!
//! **A case that dies or hangs still costs that case alone.** When a child ends early
//! the report says which case was running; the driver marks it FAILED (with how the
//! process died) or TIMED OUT, and starts a fresh child for the rest of the group.
//! `--per-case` makes every group a single case, for diagnosing a failure that might
//! depend on what ran before it in the same module.
//!
//! The group is a test module because that is the unit its author wrote — its cases
//! share helpers and fixtures, and a failure there points at one file. It is not
//! arbitrary batching, and not a coarser source module: `window` alone holds over two
//! hundred cases, which is the depth at which a shared process used to fail.
//!
//! This suite's job is that each case passes. It deliberately does NOT also answer
//! "does a long-lived process stay healthy after hundreds of windows" — a whole-suite
//! process made every verdict depend on the state every earlier case left behind, so
//! a failure landed on whichever case happened to be running, and the macOS
//! autorelease-pool abort and hang reproduced only at full-suite depth. Long-session
//! stability is a different question and needs a targeted test of its own. Grouping
//! exists because starting GTK is not free: on one macOS development host it cost
//! ~3.5 s per process (LaunchServices check-in), which made one process per case a
//! 37-minute run.
//!
//! # Run
//!
//! `xvfb-run` OUTSIDE, `dbus-run-session` INSIDE — the reverse order leaks the bus's
//! activated daemons (portal, gvfs, a11y) onto the developer's REAL X server, where they
//! outlive the run and accumulate until Xorg refuses new clients. `scripts/gtk-run.sh` carries
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
mod animation;
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
mod docref;
mod export;
mod farscroll;
mod fold;
mod forensics;
mod format;
// Test-only in `lib.rs` (gated on `test` + the GTK-suite feature); this root is
// always built `--cfg test` under that same feature, so it needs no gate here —
// but it does need the declaration, or `install_once` below drops out with
// nothing failing (`cargo xtask lint-references` check 4).
mod gtk_log_harness;
mod icons;
mod imagecache;
mod imagedecode;
mod imagefetch;
mod keynav;
mod limits;
mod lineendings;
mod links;
mod logging;
mod logrepeat;
mod macwordnav;
mod mdtable;
// Test-only in `lib.rs` (`#[cfg(test)]`); this root is always built `--cfg test`.
mod memgate;
mod notices;
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
mod taskbox;
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
// Same story as `testsymlink` above: test-only in `lib.rs`, needs the declaration here or
// the suite build breaks the moment a timing guard in it reaches the shared sampler.
mod testtiming;
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

/// The driver's private instruction to a child: run this case, by exact name.
/// Repeated once per case in the group. Not a filter — an exact name — so a child can
/// never run a body the driver did not select.
const RUN_CASE_FLAG: &str = "--run-case";

/// The driver's private instruction to a child: where to record each case's start and
/// verdict, so the driver can tell which case was running if the child dies.
const REPORT_FLAG: &str = "--report";

/// A child's exit code when at least one of its cases failed. The driver takes the
/// verdicts from the report, not from this; the code only tells a child that ran to
/// completion from one that died.
const CHILD_FAILED: i32 = 1;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let list_only = args.iter().any(|a| a == "--list");
    let Cli {
        timeout,
        skips,
        filters,
        run_cases,
        report,
        per_case,
    } = match parse_args(&args) {
        Ok(cli) => cli,
        Err(message) => {
            eprintln!("gtk_suite: {message}");
            eprintln!(
                "usage: gtk_suite [--list] [--per-case] [--skip <substring>]... \
                 [--timeout <secs>] [filter]..."
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

    if !run_cases.is_empty() {
        run_child(&cases, &run_cases, report);
    }

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

    let exe = std::env::current_exe().expect(
        "the driver re-executes its own binary for each group, so it must be able to \
         name that binary",
    );
    let report_path = std::env::temp_dir().join(format!("gtk_suite-{}.report", std::process::id()));

    // `#[ignore]` means the same thing here as it does under libtest. Reported per
    // case rather than dropped from `selected`, so the count in the summary still
    // adds up and an ignored body is visible rather than simply absent — a
    // quarantined test that vanishes from the output is how a quarantine becomes
    // permanent.
    let mut ignored = 0usize;
    for case in selected.iter().filter(|c| c.ignored) {
        println!("test {} ... ignored", case.name);
        ignored += 1;
    }
    let runnable: Vec<&Case> = selected.iter().copied().filter(|c| !c.ignored).collect();
    let groups = group_cases(&runnable, per_case);

    println!(
        "running {} of {} cases in {} group(s), one process per {} (per-case timeout {}s)\n",
        selected.len(),
        cases.len(),
        groups.len(),
        if per_case { "case" } else { "test module" },
        timeout
    );
    flush();

    let started = std::time::Instant::now();
    let mut failed: Vec<&str> = Vec::new();

    for group in &groups {
        let mut remaining: &[&Case] = group;
        while !remaining.is_empty() {
            let run = run_group_in_child(&exe, remaining, timeout, &report_path);
            let finished = run.verdicts.len();
            for (case, passed) in remaining.iter().zip(&run.verdicts) {
                if !passed {
                    failed.push(case.name);
                }
            }
            if finished == remaining.len() {
                break;
            }
            // The child ended with a case unfinished: that case is the casualty, and
            // the rest of the group gets a fresh process.
            let casualty = remaining[finished];
            if !run.casualty_started {
                // It died before announcing the case, so nothing has printed its name.
                print!("test {} ... ", casualty.name);
            }
            match run.ended {
                GroupEnd::TimedOut => {
                    println!("TIMED OUT (per-case wall-clock cap, {timeout}s)");
                }
                GroupEnd::Died(how) => println!("FAILED ({how})"),
                GroupEnd::Completed => {
                    println!("FAILED (the process exited without reporting this case's verdict)")
                }
            }
            flush();
            failed.push(casualty.name);
            remaining = &remaining[finished + 1..];
        }
    }
    let _ = std::fs::remove_file(&report_path);

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

/// The test module a case belongs to: its name without the last segment.
fn test_module(name: &str) -> &str {
    name.rsplit_once("::").map_or(name, |(module, _)| module)
}

/// Split the (name-sorted) cases into one group per test module, or one per case.
/// Sorting by name keeps a module's cases contiguous, so a module is one run of
/// the list.
fn group_cases<'a>(cases: &[&'a Case], per_case: bool) -> Vec<Vec<&'a Case>> {
    let mut groups: Vec<Vec<&'a Case>> = Vec::new();
    for case in cases {
        match groups.last_mut() {
            Some(group) if !per_case && test_module(group[0].name) == test_module(case.name) => {
                group.push(case);
            }
            _ => groups.push(vec![case]),
        }
    }
    groups
}

/// The child half: initialise GTK once in a fresh process, run the named cases in
/// order on its main thread, and record each start and verdict. Never returns.
fn run_child(cases: &[&Case], names: &[&str], report: Option<&str>) -> ! {
    // Resolve every name before initialising anything: a name the driver made up is
    // a harness defect, and must not be discovered half-way through a group.
    let resolved: Vec<&Case> = names
        .iter()
        .map(|name| {
            cases
                .iter()
                .copied()
                .find(|c| c.name == *name)
                .unwrap_or_else(|| {
                    eprintln!("gtk_suite: {RUN_CASE_FLAG} names no registered case: `{name}`");
                    std::process::exit(2);
                })
        })
        .collect();
    let mut report = report.map(|path| {
        std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .unwrap_or_else(|err| {
                eprintln!("gtk_suite: cannot open the report file `{path}`: {err}");
                std::process::exit(2);
            })
    });

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

    // This process's one-time init point for the collapsing log writer (TDD 21.13)
    // — see `gtk_log_harness`'s module docs for why a flood cannot simply be
    // piped/intercepted here, and for the libtest harness's own init point
    // (`#[gtktest::test]`'s generated wrapper), which this call has no bearing on.
    gtk_log_harness::install_once();

    let mut all_passed = true;
    for case in resolved {
        record(&mut report, "start", case.name);
        print!("test {} ... ", case.name);
        flush();
        let passed = run_case(case);
        println!("{}", if passed { "ok" } else { "FAILED" });
        flush();
        record(&mut report, if passed { "ok" } else { "FAILED" }, case.name);
        all_passed &= passed;
    }
    std::process::exit(if all_passed { 0 } else { CHILD_FAILED });
}

/// Run one body, catching its panic, and return whether it passed.
fn run_case(case: &Case) -> bool {
    let outcome = std::panic::catch_unwind(case.run);
    // `#[should_panic]` inverts the verdict, exactly as libtest inverts it: a caught
    // panic is the expected outcome (PASS) and a clean return is the failure.
    // Otherwise a `#[should_panic]` body that panics as documented has its panic
    // caught and reported FAILED, the opposite of what the author wrote.
    match (outcome, case.should_panic) {
        (Ok(()), false) | (Err(_), true) => true,
        (Ok(()), true) => {
            eprintln!(
                "    note: test {} was expected to panic, but it did not",
                case.name
            );
            false
        }
        (Err(_), false) => false,
    }
}

/// Append one `<event> <case>` line to the report and flush it, so the driver sees it
/// even if this process dies on the very next instruction.
fn record(report: &mut Option<std::fs::File>, event: &str, name: &str) {
    use std::io::Write;
    if let Some(file) = report {
        // A lost line would make the driver blame the wrong case, so a write that
        // cannot be made is fatal rather than ignored.
        if let Err(err) = writeln!(file, "{event} {name}").and_then(|()| file.flush()) {
            eprintln!("gtk_suite: cannot write the report file: {err}");
            std::process::exit(2);
        }
    }
}

/// What this runner understands on the command line.
struct Cli<'a> {
    timeout: u32,
    /// libtest's `--skip <substring>`, repeatable — cases matching any are omitted.
    skips: Vec<&'a str>,
    /// Positional substrings, libtest's filter behaviour, so muscle memory transfers.
    filters: Vec<&'a str>,
    /// Set only on a child the driver spawned: the cases, by exact name, to run.
    run_cases: Vec<&'a str>,
    /// Set only on a child the driver spawned: where to record starts and verdicts.
    report: Option<&'a str>,
    /// One process per case instead of one per test module, for diagnosis.
    per_case: bool,
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
    const VALUE_FLAGS: [&str; 4] = ["--skip", "--timeout", RUN_CASE_FLAG, REPORT_FLAG];
    let (mut timeout, mut skips, mut filters) = (DEFAULT_CASE_TIMEOUT_SECS, Vec::new(), Vec::new());
    let (mut run_cases, mut report, mut per_case) = (Vec::new(), None, false);
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
                (RUN_CASE_FLAG, Some(value)) => run_cases.push(value),
                (REPORT_FLAG, Some(value)) => report = Some(value),
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
                    // Unreachable while VALUE_FLAGS holds exactly the four arms above —
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
        } else if arg == "--per-case" {
            per_case = true;
        } else if !arg.starts_with("--") {
            filters.push(arg);
        }
        i += 1;
    }
    Ok(Cli {
        timeout,
        skips,
        filters,
        run_cases,
        report,
        per_case,
    })
}

fn flush() {
    use std::io::Write;
    let _ = std::io::stdout().flush();
}

// ── One child per group, and its per-case wall-clock cap ──────────────────────
//
// The cap lives in the DRIVER, which is what makes it portable and non-fatal. It used
// to be an `alarm(2)` inside the one shared process, which could only end a hung body
// by ending the whole suite (there was no thread to abandon it on), and did not exist
// at all off unix. A driver that waits on a child can simply kill it and move on, on
// every platform. The cap is per CASE, not per group: the deadline restarts each time
// the report shows a new case starting.
//
// Deliberately NOT capturing the child's output. The child inherits stdout and stderr,
// so its GTK warnings land in the run log in order, between `test <name> ...` and the
// verdict, exactly as they did in-process. Capturing would need a pipe and a pump to
// drain it, and a body that out-writes an undrained pipe blocks — converting a noisy
// pass into a hang. The recorded runaway (a non-terminating widget-dispose loop
// emitting 99 million repeats of one GLib warning, ~4.2 GB in ~2 minutes) is ended
// here on TIME, and its VOLUME is what `gtk_log_harness` (installed in every child)
// collapses to the first occurrence plus bounded growth milestones. The verdicts
// travel through a report FILE rather than a pipe for the same reason.

/// How a child running a group ended.
enum GroupEnd {
    /// It exited on its own, whatever the verdicts.
    Completed,
    /// Ended by something other than its own exit: a signal, an abort, a promoted
    /// critical, a Windows fast-fail. Carries a human-readable description of which.
    Died(String),
    /// A case outran the per-case cap and the driver killed the process.
    TimedOut,
}

/// What the driver learned from one child.
struct GroupRun {
    /// One verdict per case the child finished, in order — a prefix of the group.
    verdicts: Vec<bool>,
    /// Whether the child announced the first unfinished case before it ended, i.e.
    /// whether that case's `test <name> ...` line is already on stdout.
    casualty_started: bool,
    ended: GroupEnd,
}

/// How often the driver looks at a running child. Short enough to add nothing
/// noticeable per case, long enough to cost nothing.
const CHILD_POLL: std::time::Duration = std::time::Duration::from_millis(10);

fn run_group_in_child(
    exe: &std::path::Path,
    group: &[&Case],
    timeout_secs: u32,
    report_path: &std::path::Path,
) -> GroupRun {
    // Truncate: a report left by the previous child must not be read as this one's.
    if let Err(err) = std::fs::File::create(report_path) {
        return GroupRun {
            verdicts: Vec::new(),
            casualty_started: false,
            ended: GroupEnd::Died(format!("could not create the report file: {err}")),
        };
    }
    let mut command = std::process::Command::new(exe);
    command.arg(REPORT_FLAG).arg(report_path);
    for case in group {
        command.arg(RUN_CASE_FLAG).arg(case.name);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            return GroupRun {
                verdicts: Vec::new(),
                casualty_started: false,
                ended: GroupEnd::Died(format!("could not start: {err}")),
            };
        }
    };

    let cap = std::time::Duration::from_secs(u64::from(timeout_secs));
    let mut progress = read_report(report_path);
    let mut deadline = std::time::Instant::now() + cap;
    let ended = loop {
        match child.try_wait() {
            Ok(Some(status)) => break end_of(status),
            Ok(None) => {}
            Err(err) => break GroupEnd::Died(format!("could not be waited on: {err}")),
        }
        let now = read_report(report_path);
        if now.started > progress.started {
            deadline = std::time::Instant::now() + cap;
        }
        progress = now;
        if std::time::Instant::now() >= deadline {
            // Kill, then reap: an unreaped child is a zombie for the rest of the run.
            let _ = child.kill();
            let _ = child.wait();
            break GroupEnd::TimedOut;
        }
        std::thread::sleep(CHILD_POLL);
    };

    // Read once more after the exit: the last lines may have landed after the last poll.
    let progress = read_report(report_path);
    GroupRun {
        casualty_started: progress.started > progress.verdicts.len(),
        verdicts: progress.verdicts,
        ended,
    }
}

/// What a report file says so far.
struct Progress {
    /// How many cases the child has announced.
    started: usize,
    /// The verdicts it has recorded, in order.
    verdicts: Vec<bool>,
}

fn read_report(path: &std::path::Path) -> Progress {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut progress = Progress {
        started: 0,
        verdicts: Vec::new(),
    };
    // Only complete lines: a line still being written has no newline yet.
    let complete = text.rfind('\n').map_or("", |end| &text[..end]);
    for line in complete.lines() {
        match line.split_once(' ').map(|(event, _)| event) {
            Some("start") => progress.started += 1,
            Some("ok") => progress.verdicts.push(true),
            Some("FAILED") => progress.verdicts.push(false),
            _ => {}
        }
    }
    progress
}

fn end_of(status: std::process::ExitStatus) -> GroupEnd {
    match status.code() {
        Some(0 | CHILD_FAILED) => GroupEnd::Completed,
        // Hex, because a Windows death is an NTSTATUS — `0xC0000409` is the fast-fail
        // a promoted critical produces under MSVC, and nobody recognises it in decimal.
        #[cfg(windows)]
        Some(code) => GroupEnd::Died(format!("exit code {:#010X}", code as u32)),
        #[cfg(not(windows))]
        Some(code) => GroupEnd::Died(format!("exit code {code}")),
        None => GroupEnd::Died(death_by_signal(status)),
    }
}

#[cfg(unix)]
fn death_by_signal(status: std::process::ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;
    match status.signal() {
        Some(sig) => format!("killed by signal {sig}"),
        None => "ended with no exit code and no signal".to_owned(),
    }
}

#[cfg(not(unix))]
fn death_by_signal(_status: std::process::ExitStatus) -> String {
    "ended with no exit code".to_owned()
}

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
    assert!(cli.run_cases.is_empty());
    assert!(!cli.per_case);

    // The driver's instruction to a child. Its values are exact case names and a
    // path, and must never also become substring filters.
    let args = argv(&[
        REPORT_FLAG,
        "/tmp/r",
        RUN_CASE_FLAG,
        "copymap::tests::a_case",
        RUN_CASE_FLAG,
        "copymap::tests::b_case",
    ]);
    let cli = parse_args(&args).expect("the driver's own child argv must parse");
    assert_eq!(
        cli.run_cases,
        ["copymap::tests::a_case", "copymap::tests::b_case"]
    );
    assert_eq!(cli.report, Some("/tmp/r"));
    assert!(cli.filters.is_empty(), "{:?}", cli.filters);
    assert!(parse_args(&argv(&["--per-case"])).expect("parses").per_case);
    assert!(
        parse_args(&argv(&[RUN_CASE_FLAG])).is_err(),
        "a child told to run no case must be rejected, not run nothing and exit 0"
    );

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

/// The driver decides which case a dead child was running from the report alone, so
/// a misread report blames the wrong case — and a line still being written when the
/// driver reads it is the ordinary case, not an edge one.
#[gtktest::test]
fn the_report_counts_only_complete_lines_and_groups_follow_test_modules() {
    let path =
        std::env::temp_dir().join(format!("gtk_suite-selftest-{}.report", std::process::id()));
    std::fs::write(
        &path,
        "start a::t::one\nok a::t::one\nstart a::t::two\nFAILED a::t::two\nstart a::t::thr",
    )
    .expect("write the fixture report");
    let progress = read_report(&path);
    let _ = std::fs::remove_file(&path);
    assert_eq!(
        progress.started, 2,
        "a start line with no newline yet must not count as started"
    );
    assert_eq!(progress.verdicts, [true, false]);

    assert_eq!(test_module("a::b::tests::case"), "a::b::tests");
    assert_eq!(test_module("bare"), "bare");
}
