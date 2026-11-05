//! Crash report files: naming them, writing the panic-time one, and noticing on the
//! next launch that one is unread.
//!
//! The fatal-signal report is written by `signal.rs` instead, byte for byte in the
//! same order but with async-signal-safe primitives. **The order is the contract**
//! (TDD 21.6): identity and fault, then breadcrumbs, then the backtrace and module
//! map. A report truncated by a double fault is therefore still worth having, and
//! the fields most likely to be lost are the ones a reader needs least.

use std::io::Write;
use std::path::{Path, PathBuf};

use super::ring::Ring;
use super::timefmt;

/// Prefix and extension of a crash report, shared by the writer and the
/// next-launch scan so a rename cannot desynchronise them.
const REPORT_PREFIX: &str = "crash-";
const REPORT_SUFFIX: &str = ".log";

/// Name of the marker recording the newest report already announced, so TDD 21.9's
/// "exactly once" survives a restart. A plain file holding one filename: the state
/// it protects is one string, and a crash-forensics feature that needed the session
/// machinery to work would be a feature that stops working in the cases it exists
/// for.
pub(crate) const SEEN_MARKER_NAME: &str = "crash-last-seen";

/// First line of every marker this version writes, and the thing that makes the format
/// say which format it is.
///
/// **The ambiguity this removes was a live defect, not a tidiness question** (QA round 5,
/// M-2). The marker had two meanings and no way to tell them apart: the pre-set format
/// held ONE name meaning "this and everything older", and the set format holds one name
/// per line — so a set with exactly one member, which is the steady state on any machine
/// that has crashed once, was indistinguishable from a legacy watermark. `seen_set` read
/// it as the watermark, and that reading marks every present report sorting at or below it
/// as already announced. Which is round 4's backward-clock bug exactly: the next crash
/// after an NTP correction, a local-time RTC or a restored VM snapshot writes a report
/// whose name sorts BELOW the marker, and it was silently discarded as seen.
///
/// The old comment argued the ambiguity was safe because "reading it as a watermark can
/// only ever mark MORE reports as seen, and a missed announcement is a nuisance where a
/// repeated one erodes trust". That is a sound argument about which way to resolve an
/// ambiguity you are stuck with. It is not an argument for staying stuck with it: the
/// harm it accepts is precisely the harm round 4 was fixed to stop accepting, and a
/// header line costs nothing. A legacy marker still gets the safe reading, because for
/// one of those the ambiguity is genuinely unresolvable.
const SEEN_MARKER_HEADER: &str = "# scribobulate: crash reports already announced";

/// The report file name for a run started at `started` with process id `pid`.
///
/// The timestamp is the run's **start**, not the crash instant, and deliberately:
/// the name is computed at install time so the signal handler has nothing to format
/// and no memory to reach for — it opens a path that was already a NUL-terminated
/// byte string before anything went wrong. The crash instant is written *inside*
/// the report. The pid keeps two concurrent instances (`--new-instance`, the test
/// suite) from colliding.
pub(crate) fn report_file_name(started_unix_millis: i64, pid: u32) -> String {
    let mut stamp = String::new();
    let _ = timefmt::civil_from_unix_millis(started_unix_millis).write_compact(&mut stamp);
    format!("{REPORT_PREFIX}{stamp}-{pid}{REPORT_SUFFIX}")
}

/// Whether `name` is a crash report rather than some other file in the state dir.
fn is_report(name: &str) -> bool {
    name.starts_with(REPORT_PREFIX) && name.ends_with(REPORT_SUFFIX)
}

/// The newest report in `names` that is not in `seen`.
///
/// **Membership, not a `>` watermark, and that is the whole point.** This compared
/// `*n > last_seen` until QA round 4. Lexicographic order over these names IS
/// chronological order — the fixed-width `YYYYMMDDThhmmssZ` stamp leads the
/// variable-width pid, which is why `timefmt`'s compact form exists — but only while
/// the wall clock moves forward, and nothing guarantees that. An NTP correction, a
/// dual-boot machine with a local-time RTC, or a restored VM snapshot steps it
/// backwards, and every report written after the step sorts BELOW the marker. The
/// comparison then silently discards them: TDD 21.9's "a subsequent launch does not
/// repeat it" kept working while its "the next launch notices" half stopped, for as
/// long as it took real time to climb back past the marker — indefinitely, if the
/// clock was wrong by years rather than seconds.
///
/// Set membership needs no such assumption. Ordering is still used to pick the
/// *newest* of several unread reports, which is a presentation choice: if the clock
/// misbehaves the worst outcome is announcing them in an odd order, not dropping
/// them. Pure, so both halves of 21.9 are testable without crashing anything.
pub(crate) fn unread_report<'a>(names: &[&'a str], seen: &[&str]) -> Option<&'a str> {
    names
        .iter()
        .copied()
        .filter(|n| is_report(n))
        .filter(|n| !seen.contains(n))
        .max()
}

/// Parse the seen-marker into the set of report names already announced.
///
/// Three shapes, told apart by [`SEEN_MARKER_HEADER`] rather than by counting lines:
///
/// * **Headed** (everything this version writes) — a set, whatever its size. A
///   one-element set is now expressible, which is the whole point: it is the steady
///   state after a single crash, and reading it as a watermark re-armed round 4's
///   backward-clock bug (M-2).
/// * **Headless, several lines** — the interim set format. Unambiguous already: a
///   watermark was never more than one line.
/// * **Headless, one line** — a legacy watermark, and the one case where the ambiguity
///   is real and unresolvable. Migrating it faithfully needs the directory listing, so
///   it expands to every present report at or below the watermark: exactly the old
///   predicate's answer, evaluated once. Without this the first launch after upgrading
///   would re-announce every report ever kept. It keeps the safe-direction reading
///   because for this shape there is nothing better available — not because the
///   ambiguity was acceptable.
///
/// **Every shape is pruned to reports that still exist**, which is what bounds the
/// marker: a deleted report can never be announced again, so remembering it is pure
/// growth, and the marker tracks the report directory's size rather than the machine's
/// lifetime crash count.
///
/// That pruning used to happen only in the watermark branch — where it is structural,
/// since a watermark is *evaluated* against `present` — and the set branches returned
/// their lines unpruned. It went unnoticed because a one-line marker took the watermark
/// branch, and a one-line marker is the steady state; the bound the writer's comment
/// claimed was being delivered by the very ambiguity M-2 removed. Fixing the ambiguity
/// routed the steady state to the set branch and the marker started growing, which is
/// how this surfaced: `the_marker_forgets_reports_that_no_longer_exist` failed. Worth
/// recording as a shape rather than an incident — a property implemented on one branch
/// of a discriminator and *documented* as a property of the whole function survives
/// exactly until the discriminator changes.
fn seen_set<'a>(marker: Option<&'a str>, present: &[&'a str]) -> Vec<&'a str> {
    let Some(marker) = marker else {
        return Vec::new();
    };
    let extant = |n: &&'a str| is_report(n) && present.contains(n);
    let mut lines = marker.lines().map(str::trim).filter(|l| !l.is_empty());
    let first = lines.next();
    if first == Some(SEEN_MARKER_HEADER) {
        return lines.filter(extant).collect();
    }
    let lines: Vec<&str> = first.into_iter().chain(lines).collect();
    if lines.len() == 1 {
        let watermark = lines[0];
        return present
            .iter()
            .copied()
            .filter(|n| is_report(n) && *n <= watermark)
            .collect();
    }
    lines.into_iter().filter(extant).collect()
}

/// Render the seen set as marker text — the one place the on-disk format is decided,
/// so the writer and [`seen_set`] cannot drift apart into two different formats.
fn seen_marker_text(seen: &[&str]) -> String {
    let mut out = String::from(SEEN_MARKER_HEADER);
    for name in seen {
        out.push('\n');
        out.push_str(name);
    }
    out
}

/// Announce an unread crash report, once (TDD 21.9).
///
/// Deliberately a `warn!` and not a dialog. The report exists to be handed over on
/// request; interrupting a launch with a modal to say something already went wrong
/// hours ago would be the tail wagging the dog — and the announcement itself lands
/// in the persistent log, so it is recoverable even if nobody was watching stderr.
pub(crate) fn announce_unread_report(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();

    let marker = dir.join(SEEN_MARKER_NAME);
    let marker_text = std::fs::read_to_string(&marker).ok();
    let mut seen = seen_set(marker_text.as_deref(), &borrowed);
    let Some(newest) = unread_report(&borrowed, &seen) else {
        return;
    };

    log::warn!(
        "a crash report from a previous run is unread: {} — it names the fault, \
         the build, and the last {} things the application did before it died",
        dir.join(newest).display(),
        super::ring::CAPACITY
    );
    // Written before the next launch can repeat the announcement. A failure here
    // costs a duplicate warning, which is why it is not worth reporting.
    //
    // `seen` arrives already pruned to reports that still EXIST — see `seen_set`, which
    // owns that bound for every marker shape. Naming the property here and implementing
    // it there is what let it be true of only one branch for a whole round.
    seen.push(newest);
    seen.sort_unstable();
    seen.dedup();
    // Through `private_options`, not `std::fs::write` (QA round 5, L-9). This was the
    // fifth writer in the state directory to omit the mode — the omission `mod.rs`'s own
    // doc comment predicted would keep happening at any site that has to remember it.
    // The marker names the reports on this machine and when they happened; it is the
    // same class of information as the reports themselves and gets the same 0600.
    // `truncate` is right here, unlike in `write_report`: a marker is current state, not
    // accumulated evidence.
    let _ = super::private_options()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&marker)
        .and_then(|mut f| f.write_all(seen_marker_text(&seen).as_bytes()));
}

/// Write a crash report. The one writer both the panic hook and the tests use;
/// `signal.rs` mirrors this layout with `write(2)`.
pub(crate) fn write_report(
    path: &Path,
    kind: &str,
    header: &str,
    fault: &str,
    ring: &Ring,
    backtrace: Option<&str>,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        crate::session::create_state_dir(parent)?;
    }
    // Not `File::create`: that is `write(true).create(true).truncate(true)` with no
    // mode, and it is the call that actually creates this path in the panic case —
    // silencing the signal handler's `0600` (see `super::private_options`).
    //
    // APPEND, NOT TRUNCATE, and this is the half that is easy to get wrong (QA round 5,
    // H-2). A panic inside a gtk-rs `extern "C"` trampoline cannot unwind across the C
    // frame — glib 0.21.5 puts no `catch_unwind` there — so the runtime raises a SECOND
    // panic, "panic in a function that cannot unwind", and *this hook runs again*. With
    // `truncate` the second invocation destroyed the good report before `abort()` had even
    // been reached, so the fatal-signal writer was never the only destroyer and fixing
    // `signal.rs` alone would have closed this defect without fixing it.
    //
    // Appending is safe precisely because the path is per-run: `report_file_name` embeds
    // the run's start timestamp and the pid, so nothing from an earlier run is at this
    // path to be grown. Within one dying run, every fault is evidence and each report
    // carries its own `=== scribobulate crash report ===` banner, so a concatenated file
    // reads as the sequence of faults it is — the first (and most informative) one first,
    // which is the same reasoning as TDD 21.6's field order.
    let mut file = super::private_options()
        .append(true)
        .create(true)
        .open(path)?;

    // 1. Identity and fault — everything a report needs to be worth reading.
    write!(file, "=== scribobulate crash report ===\n\nkind: {kind}\n")?;
    writeln!(file, "crashed: {}", timefmt::now_iso8601())?;
    file.write_all(header.as_bytes())?;
    write!(file, "\n{fault}\n")?;

    // 2. What the application was doing — the deliverable (TDD 21.5).
    writeln!(file, "\n--- breadcrumbs, oldest first ---")?;
    let mut wrote_any = false;
    ring.for_each(|line| {
        wrote_any = true;
        let _ = file.write_all(line);
        let _ = file.write_all(b"\n");
    });
    if !wrote_any {
        writeln!(file, "(none recorded)")?;
    }

    // 3. Last, because it is the least useful half on this platform: the shipped
    //    binary is stripped and the distribution's GTK carries no symbols
    //    (GEP-15), so these frames resolve only as module + offset.
    if let Some(backtrace) = backtrace {
        writeln!(file, "\n--- backtrace ---\n{backtrace}")?;
    }
    file.flush()
}

/// Install the panic hook (TDD 21.7).
///
/// Chains to the previous hook rather than replacing it, so the panic message still
/// reaches stderr and `RUST_BACKTRACE` still behaves. The hook only *observes* —
/// unwinding continues, `Drop` still runs, and `Cargo.toml`'s deliberate
/// `panic = "unwind"` stays meaningful.
///
/// **A panic `richimg` will itself catch and contain writes NO crash report.** This
/// hook fires on EVERY panic, caught or not — that is how a hook works — so without
/// this check a decoder panic `richimg::guarded` is about to convert into
/// `Error::DecoderPanicked` (the caller gets an ordinary error; the process keeps
/// running) would still write a crash report **naming a crash that did not happen**.
/// [`richimg::contained_panic_in_progress`] is true for the duration of such a call,
/// including while the panic is unwinding through it — i.e. exactly when this hook
/// runs — so it is checked first and the panic is logged instead (`richimg`
/// contains every decoder panic and reports it as an ordinary error).
pub(crate) fn install_panic_hook(
    report_path: Option<PathBuf>,
    header: String,
    ring: &'static Ring,
) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = panic_message(info);
        let location = info
            .location()
            .map(ToString::to_string)
            .unwrap_or_else(|| "(unknown)".to_owned());
        if richimg::contained_panic_in_progress() {
            log::error!("contained decoder panic: {message} at {location}");
        } else if let Some(path) = report_path.as_deref() {
            let fault = format!("panic: {message}\nlocation: {location}");
            // A panic *inside* the hook would abort the process and destroy the
            // very evidence being written, so every step here is fallible-and-
            // ignored rather than `unwrap`ed.
            let backtrace = std::backtrace::Backtrace::force_capture().to_string();
            let _ = write_report(path, "panic", &header, &fault, ring, Some(&backtrace));
        }
        previous(info);
    }));
}

/// The panic payload as text, for the two shapes `panic!` can produce.
fn panic_message(info: &std::panic::PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "(non-string panic payload)".to_owned()
    }
}

#[cfg(test)]
mod tests {

    /// A crash report must be owner-only, and the assertion has to be made in the
    /// ORDER that actually defeats it.
    ///
    /// The fatal-signal handler passes `0600` to `open(2)`, but that mode applies only
    /// when the call CREATES the file. The panic hook writes the same path first, so
    /// the report exists by the time the handler runs and the handler's mode is a
    /// no-op. A test that checks a freshly-created report is 0600 passes on the broken
    /// code — the defect only appears on the second writer.
    ///
    /// So this writes the report TWICE and asserts after each: once for the creating
    /// path, once for the pre-existing path. Measured on the pre-fix code: 0664 under a
    /// umask of 0002 — not 0644, because the leak is whatever the umask gives, which is
    /// why the constant here is the intent and never an observed value.
    ///
    /// Be precise about which assertion earns its place, since overstating that is the
    /// habit this round has been about. **The first one kills the real mutant** — revert
    /// the creator to `File::create` and it fails immediately. The second cannot fail
    /// while a single seam serves both writers, and is here to pin the ORDERING the
    /// system actually got wrong (panic creates, signal rewrites): it fails the moment
    /// anyone re-splits the seam so that creation states a mode and rewriting does not,
    /// which is exactly the topology that shipped.
    /// TDD 21.12, and it is **compiled on every platform** (QA round 5, M-6).
    ///
    /// It used to be `#[cfg(unix)]`, which does not skip a test on Windows — it
    /// deletes it. Not compiled, not reported, not counted, and no harness has a
    /// column in which "never built" differs from "passed", so the suite was green
    /// over a rubric it never ran. That is the exact rule `src/testsymlink.rs` was
    /// added in this same commit to encode, unapplied to the rubric shipped beside it.
    ///
    /// The assertion really is unix-shaped — "owner-only" is a mode on unix and an ACL
    /// on Windows — so the platform *exclusion* is replaced by a platform-appropriate
    /// path plus an explicit, greppable skip that names the rubric left unverified.
    #[test]
    fn a_crash_report_is_owner_only_even_when_it_already_exists() {
        #[cfg(not(any(unix, windows)))]
        {
            crate::testsymlink::skipped(
                "TDD 21.12 crash-report permissions",
                "owner-only has no implementation on this platform",
            );
        }
        // Windows: owner-only is the state directory's protected DACL, which a report
        // inherits (`platform::win32::privacy`). Same two-writer order as unix — the panic
        // hook creates, the native-crash writer appends — and the second writer must not
        // leave the report with an ACL of its own. `forensics::exception`'s death tests
        // hold the same line for the file the crash handler itself creates.
        #[cfg(windows)]
        {
            let dir = tempfile::tempdir().expect("tempdir");
            let state = dir.path().join("state");
            let path = state.join("crash.txt");
            let ring = Ring::new();
            let check = |when: &str| {
                if let Err(why) = crate::platform::win32::inherits_only_from(&path, &state) {
                    panic!("the report after the {when} write is not owner-only: {why}");
                }
            };
            write_report(&path, "panic", "hdr\n", "fault", &ring, None).expect("first write");
            check("creating");
            write_report(&path, "exception", "hdr\n", "fault", &ring, None).expect("second write");
            check("second");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("crash.txt");
            let ring = Ring::new();

            let mode_now = || {
                std::fs::metadata(&path)
                    .expect("report exists")
                    .permissions()
                    .mode()
                    & 0o777
            };

            write_report(&path, "panic", "hdr\n", "fault", &ring, None).expect("first write");
            assert_eq!(mode_now(), 0o600, "the CREATING write must be owner-only");

            // The half that regressed: the path now exists, so any creator that does not
            // state a mode leaves the previous permissions in place.
            write_report(&path, "signal", "hdr\n", "fault", &ring, None).expect("second write");
            assert_eq!(
                mode_now(),
                0o600,
                "a report rewritten over an EXISTING file must still be owner-only — the \
             ordering that defeated the handler's own 0600, and the one a fresh-file \
             test cannot express"
            );
        }
    }
    use super::*;

    #[test]
    fn a_report_name_carries_the_run_start_and_the_pid() {
        assert_eq!(
            report_file_name(1_785_290_079_123, 1_227_729),
            "crash-20260729T015439Z-1227729.log"
        );
    }

    #[test]
    fn report_names_sort_chronologically() {
        // What makes `unread_report`'s `max` correct.
        let earlier = report_file_name(1_785_290_079_000, 999);
        let later = report_file_name(1_785_290_080_000, 1);
        assert!(earlier < later, "{earlier} !< {later}");
    }

    #[test]
    fn the_newest_unseen_report_is_the_one_announced() {
        let names = [
            "crash-20260729T015439Z-1.log",
            "crash-20260730T101010Z-2.log",
            "scribobulate.log",
            "session.toml",
        ];
        assert_eq!(
            unread_report(&names, &[]),
            Some("crash-20260730T101010Z-2.log")
        );
    }

    #[test]
    fn an_already_seen_report_is_not_announced_again() {
        // TDD 21.9's "exactly once".
        let names = ["crash-20260729T015439Z-1.log"];
        assert_eq!(
            unread_report(&names, &["crash-20260729T015439Z-1.log"]),
            None
        );
    }

    #[test]
    fn a_report_newer_than_the_marker_is_still_announced() {
        let names = [
            "crash-20260729T015439Z-1.log",
            "crash-20260731T090000Z-7.log",
        ];
        assert_eq!(
            unread_report(&names, &["crash-20260729T015439Z-1.log"]),
            Some("crash-20260731T090000Z-7.log")
        );
    }

    /// A report written after the wall clock steps BACKWARD is still announced.
    ///
    /// The regression QA round 4 found. The marker held a watermark and the predicate
    /// was `name > watermark`, so a report whose name sorts below an already-announced
    /// one was discarded as "already seen" — even though it had never been announced
    /// and had not existed when the marker was written. An NTP correction, a
    /// local-time RTC on a dual-boot box, or a restored VM snapshot all produce this.
    ///
    /// Note what the old test suite asserted and why it could not catch this: every
    /// case moved the clock FORWARD. "Newer than the marker is announced" and "equal
    /// to the marker is not" are both true of a broken implementation; the population
    /// the tests ranged over excluded the only inputs that discriminate (GEP-5).
    ///
    /// **And note what THIS test's first version could not catch either** (QA round 5,
    /// M-2, GEP-1). It built the marker by hand and passed `present = [announced]`
    /// — omitting `after_step`, the only report whose classification is in question. The
    /// legacy-watermark branch filters `present`, so with the discriminating report left
    /// out of it there was nothing for the wrong reading to wrongly exclude, and the test
    /// passed over an implementation that discarded the report in production. A setup
    /// that removes the input under test guarantees its own pass.
    ///
    /// So this now goes through the real writer and the real directory listing:
    /// `announce_unread_report` writes the marker, the second report lands beside the
    /// first, and `present` is both of them — the shape production actually has.
    /// Mutation guard: drop `SEEN_MARKER_HEADER` from `seen_marker_text` and this fails,
    /// because the one-line marker falls back to the watermark reading.
    #[test]
    fn a_report_written_after_the_clock_steps_backward_is_still_announced() {
        let dir = tempfile::tempdir().unwrap();
        let announced = "crash-20260731T090000Z-7.log";
        // Same machine, next crash, clock now two days behind: the name sorts BELOW
        // the marker even though this report is strictly newer in reality.
        let after_step = "crash-20260729T015439Z-9.log";

        std::fs::write(dir.path().join(announced), "report").unwrap();
        announce_unread_report(dir.path()); // first launch: announces, writes the marker
        std::fs::write(dir.path().join(after_step), "report").unwrap();

        let marker = std::fs::read_to_string(dir.path().join(SEEN_MARKER_NAME)).unwrap();
        // Exactly what the next launch sees: every file in the directory.
        let names = [announced, after_step];
        let seen = seen_set(Some(marker.as_str()), &names);

        assert_eq!(
            unread_report(&names, &seen),
            Some(after_step),
            "a report that did not exist when the marker was written has never been \
             announced, whatever its name sorts as"
        );
    }

    /// The steady-state marker — one crash, one name — is a SET, not a watermark.
    ///
    /// The single case that discriminates the two readings, and the one the format was
    /// ambiguous about (M-2). Everything a machine that has crashed exactly once does
    /// goes through here.
    #[test]
    fn a_one_element_marker_is_a_set_and_not_a_watermark() {
        let announced = "crash-20260731T090000Z-7.log";
        let older_but_unannounced = "crash-20260729T015439Z-9.log";
        let present = [announced, older_but_unannounced];

        let written = seen_marker_text(&[announced]);
        let seen = seen_set(Some(&written), &present);
        assert_eq!(
            seen,
            vec![announced],
            "a headed one-line marker names exactly the report it names — reading it as \
             a watermark silently swallows every older report that was never announced"
        );

        // The legacy shape, which genuinely is a watermark, still reads as one.
        let legacy = seen_set(Some(announced), &present);
        assert!(
            legacy.contains(&older_but_unannounced),
            "a HEADLESS one-line marker predates the set format and must keep its \
             watermark meaning, or upgrading re-announces every old report"
        );
    }

    /// The legacy one-line marker keeps meaning "this and everything older".
    ///
    /// Migration matters more than it looks: read as a one-element SET, an existing
    /// marker would make every older report on every installed machine unread again,
    /// and the first launch after upgrading would announce a crash from months ago.
    #[test]
    fn a_legacy_watermark_marker_does_not_resurrect_older_reports() {
        let old = "crash-20260728T000000Z-1.log";
        let watermark = "crash-20260729T015439Z-1.log";
        let newer = "crash-20260730T101010Z-2.log";
        let names = [old, watermark, newer];

        let seen = seen_set(Some(watermark), &names);
        assert!(
            seen.contains(&old),
            "older than the watermark counts as seen"
        );
        assert!(
            seen.contains(&watermark),
            "the watermark itself counts as seen"
        );
        assert_eq!(
            unread_report(&names, &seen),
            Some(newer),
            "only the genuinely unannounced report is announced"
        );
    }

    /// The marker tracks extant reports, not the machine's lifetime crash count.
    ///
    /// This is the test that caught the pruning being implemented on only one branch of
    /// `seen_set` (see its doc comment). It ranges over the whole marker lifecycle —
    /// announce, delete, crash again, announce — rather than over a parsed string, which
    /// is why it noticed when the steady state changed branches underneath it.
    #[test]
    fn the_marker_forgets_reports_that_no_longer_exist() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("crash-20260729T015439Z-1.log");
        std::fs::write(&first, "report").unwrap();
        announce_unread_report(dir.path());

        // The user hands the report over and deletes it; a later crash writes another.
        std::fs::remove_file(&first).unwrap();
        std::fs::write(dir.path().join("crash-20260730T101010Z-2.log"), "report").unwrap();
        announce_unread_report(dir.path());

        let marker = std::fs::read_to_string(dir.path().join(SEEN_MARKER_NAME)).unwrap();
        assert_eq!(
            marker,
            seen_marker_text(&["crash-20260730T101010Z-2.log"]),
            "a deleted report cannot be re-announced, so remembering it is pure growth"
        );
    }

    /// The marker is as private as the reports it indexes (L-9), on every platform
    /// that has file modes and reported as unverified on those that do not (M-6).
    #[test]
    fn the_seen_marker_is_owner_only() {
        #[cfg(not(unix))]
        {
            crate::testsymlink::skipped(
                "TDD 21.12 seen-marker permissions",
                "file modes are a unix concept",
            );
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("crash-20260729T015439Z-1.log"), "report").unwrap();
            announce_unread_report(dir.path());
            let mode = std::fs::metadata(dir.path().join(SEEN_MARKER_NAME))
                .expect("marker written")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "the seen marker names this machine's crashes");
        }
    }

    #[test]
    fn a_directory_with_no_reports_announces_nothing() {
        assert_eq!(unread_report(&["scribobulate.log"], &[]), None);
    }

    #[test]
    fn announcing_writes_a_marker_that_silences_the_second_launch() {
        let dir = tempfile::tempdir().unwrap();
        let report = dir.path().join("crash-20260729T015439Z-1.log");
        std::fs::write(&report, "report").unwrap();

        announce_unread_report(dir.path());
        let marker = std::fs::read_to_string(dir.path().join(SEEN_MARKER_NAME)).unwrap();
        assert_eq!(marker, seen_marker_text(&["crash-20260729T015439Z-1.log"]));

        // Second launch: the same directory now yields nothing to announce.
        let names = ["crash-20260729T015439Z-1.log"];
        let seen = seen_set(Some(marker.as_str()), &names);
        assert_eq!(unread_report(&names, &seen), None);
    }

    #[test]
    fn a_written_report_leads_with_identity_and_fault_and_ends_with_the_backtrace() {
        // TDD 21.6: the order is the contract — a report truncated by a double
        // fault must still name the build and the signal.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash.log");
        let ring = Ring::new();
        ring.record("2026-07-29T01:54:00.000Z INFO  scribobulate::app::open: opened /tmp/a.md");
        ring.record("2026-07-29T01:54:38.000Z INFO  scribobulate::window::reload: monitor event");

        write_report(
            &path,
            "SIGSEGV",
            "scribobulate 0.1.0 (0badc0de, release)\npid: 1227729\n",
            "signal: SIGSEGV\nfault address: 0x30",
            &ring,
            Some("frame #0 …"),
        )
        .unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let identity = text.find("scribobulate 0.1.0").unwrap();
        let fault = text.find("fault address: 0x30").unwrap();
        let crumbs = text.find("opened /tmp/a.md").unwrap();
        let backtrace = text.find("frame #0").unwrap();
        assert!(
            identity < fault && fault < crumbs && crumbs < backtrace,
            "wrong order in:\n{text}"
        );
        assert!(text.contains("monitor event"));
    }

    /// **H-2.** A second fault must not destroy the report the first one wrote.
    ///
    /// This hook fires more than once per dying process, and that was the whole defect.
    /// A panic inside a gtk-rs `extern "C"` trampoline cannot unwind across the C frame,
    /// so the runtime raises a second panic — "panic in a function that cannot unwind" —
    /// and re-enters this same hook. While `write_report` truncated, that second, nearly
    /// contentless invocation erased the first report's panic message, location and
    /// backtrace *before* `abort()` was reached. The `O_APPEND` fix on the fatal-signal
    /// writer alone would not have touched this: the evidence was already gone by the
    /// time `SIGABRT` was raised.
    ///
    /// Mutation guard: put `.truncate(true)` back in place of `.append(true)` and the
    /// first-report assertions below fail. Asserted on the FIRST report's distinctive
    /// content rather than on file length, because length grows under either bug.
    #[test]
    fn a_second_fault_does_not_destroy_the_first_faults_report() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash.log");
        let ring = Ring::new();
        ring.record("the breadcrumb that explains everything");

        write_report(
            &path,
            "panic",
            "scribobulate 0.1.0 (test)\n",
            "panic: the original fault\nlocation: src/real/culprit.rs:42",
            &ring,
            Some("frame #0 the_original_frame"),
        )
        .expect("first report");

        // The runtime's second panic: it knows nothing useful, and under the old code it
        // was the only thing that survived.
        write_report(
            &path,
            "panic",
            "scribobulate 0.1.0 (test)\n",
            "panic: panic in a function that cannot unwind\nlocation: (unknown)",
            &ring,
            None,
        )
        .expect("second report");

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("panic: the original fault"),
            "the second fault destroyed the first fault's message:\n{text}"
        );
        assert!(
            text.contains("src/real/culprit.rs:42"),
            "the second fault destroyed the first fault's location:\n{text}"
        );
        assert!(
            text.contains("frame #0 the_original_frame"),
            "the second fault destroyed the first fault's backtrace:\n{text}"
        );
        // And the second fault is recorded too — appending keeps both, in order.
        assert!(
            text.contains("panic in a function that cannot unwind"),
            "{text}"
        );
        let first = text.find("panic: the original fault").unwrap();
        let second = text.find("cannot unwind").unwrap();
        assert!(
            first < second,
            "faults must read oldest-first, like the fields within one report (TDD 21.6)"
        );
    }

    #[test]
    fn a_report_with_no_breadcrumbs_says_so_rather_than_looking_truncated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash.log");
        write_report(&path, "panic", "header\n", "fault\n", &Ring::new(), None).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("(none recorded)"), "{text}");
    }

    /// Install the production panic hook, then FENCE it to the calling thread, and
    /// hand back the teardown that removes both.
    ///
    /// # Two hazards, two mechanisms
    ///
    /// A panic hook is process-global in two independent ways, and this helper answers
    /// both: the FENCE keeps another *thread's* panic out of this test's hook, and
    /// [`hook_lock`] keeps another hook-installing *test* from tearing this one's hook
    /// down mid-test. Removing either brings back its own distinct intermittent
    /// failure, on its own distinct assertion.
    ///
    /// # Why a fence, and why here
    ///
    /// This is the problem [`crate::testlog`] already solves for the log sink, and the
    /// same answer: scope a process-global to the thread libtest gave this test
    /// exclusively. A panic hook is process-global and fires on EVERY panic on EVERY
    /// thread — that is how a hook works, and the production hook depends on it. But
    /// both tests below assert on what the hook did with a *specific* panic, and one
    /// of them asserts ABSENCE. libtest meanwhile runs the rest of the suite in
    /// parallel, and several of those tests panic on purpose —
    /// `docio::a_panic_on_the_pool_thread_is_re_raised_not_swallowed` panics on a GLib
    /// pool thread precisely to prove the report path survives it. Unfenced, that
    /// panic runs under whichever hook is installed at that instant, writes a crash
    /// report into THIS test's temp directory, and the absence assertion fails
    /// naming a report neither test wrote.
    ///
    /// MEASURED on the pre-fix code, arms run against one binary whose identity was
    /// checked (this helper's own test does not exist there): **0 failures in 15 runs
    /// idle at full parallelism, 1 in 10 under CPU load.** So the enabler is CONTENTION,
    /// not the thread count — parallelism supplies the opportunity and load supplies the
    /// duration, and an idle rerun is not a verdict on either. That is the worst shape a
    /// flake can take: it passes every casual retry and reddens the build pipeline.
    ///
    /// # What the fence does NOT weaken
    ///
    /// It wraps the production hook rather than standing in for it: a panic on this
    /// thread runs [`install_panic_hook`]'s own closure, unmodified, so what is under
    /// test is still the real thing end to end. A panic on any other thread reaches
    /// the hook that was installed before this test began, so a concurrent test's
    /// output and backtrace are exactly what they would have been.
    fn install_panic_hook_fenced_to_this_thread(
        report_path: Option<PathBuf>,
        header: String,
        ring: &'static Ring,
    ) -> impl FnOnce() {
        type Hook = std::sync::Arc<dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send + 'static>;

        // Held until the teardown runs. See "Two hazards, two mechanisms" above: the
        // fence answers a foreign THREAD's panic, this answers a concurrent
        // hook-installing TEST, and neither substitutes for the other.
        let serialised = hook_lock().lock().unwrap_or_else(|e| e.into_inner());

        // The hook libtest (or an outer test) had installed. Kept behind an `Arc` so
        // it can be both chained into the production hook and used by the fence's
        // other-thread arm — a `Box<dyn Fn>` cannot be cloned.
        let outer: Hook = std::sync::Arc::from(std::panic::take_hook());
        {
            let outer = std::sync::Arc::clone(&outer);
            std::panic::set_hook(Box::new(move |info| outer(info)));
        }

        // Production installs over that, chaining to it exactly as it does in the app.
        install_panic_hook(report_path, header, ring);
        let production: Hook = std::sync::Arc::from(std::panic::take_hook());

        let owner = std::thread::current().id();
        let other = std::sync::Arc::clone(&outer);
        std::panic::set_hook(Box::new(move |info| {
            if std::thread::current().id() == owner {
                production(info);
            } else {
                other(info);
            }
        }));

        move || {
            let _ = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| outer(info)));
            drop(serialised);
        }
    }

    /// Serialises every test that installs a panic hook, exactly as
    /// [`crate::testlog`]'s own lock serialises every test that captures log records —
    /// and for a hazard the thread fence above cannot cover.
    ///
    /// `std::panic::take_hook` + `set_hook` is a read-modify-write of one process-wide
    /// slot. Two tests doing it concurrently interleave destructively: the second one's
    /// *teardown* restores the hook it saw on the way in, silently discarding the first
    /// one's still-live hook. The first test then runs its subject panic with no hook of
    /// its own installed, and every assertion about what that hook did reports that it
    /// did nothing.
    ///
    /// **That mechanism is ANALYSED, not measured, and the distinction is kept on
    /// purpose.** What is MEASURED is the symptom: with the thread fence alone this
    /// suite still failed **3 runs in 8** under load, on a DIFFERENT assertion than
    /// before (`the contained panic must still be logged … []`, an empty capture). The
    /// production hook can only log nothing on a contained panic if it was not installed
    /// — which is where the mechanism above comes from. It is inference from the failure
    /// text, and it is good inference, but it is not isolation.
    ///
    /// **Isolation was attempted and did not work**, so do not read a clean negative as
    /// evidence there is nothing here: the two hook-installing tests run as a PAIR
    /// failed 0 times in 15 under the same load, as did every three-test subset tried.
    /// Only the full suite reproduces it. A two-test run finishes in microseconds and is
    /// structurally too small to produce the overlap the defect needs — the rig cannot
    /// express the property, which is knowable before running it. What remains unproven
    /// is whether this hazard CAUSED the original failures or is a second real defect
    /// fixed alongside them; both mechanisms are cheap, so both stay either way.
    fn hook_lock() -> &'static std::sync::Mutex<()> {
        static L: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        L.get_or_init(|| std::sync::Mutex::new(()))
    }

    /// The fence's own regression guard — the positive control for
    /// [`install_panic_hook_fenced_to_this_thread`], which is otherwise a piece of
    /// machinery whose failure mode is *another test* going red intermittently.
    ///
    /// Reproduces the exact collision deterministically instead of waiting for the
    /// scheduler to produce it: a panic on another thread, raised while this thread
    /// holds a report-writing hook. That is `docio`'s pool-thread panic in miniature,
    /// and before the fence it wrote a crash report into the temp directory below.
    ///
    /// Delete the fence and this fails every run, which is the point — the flake it
    /// replaces failed about one run in three, and only under load.
    #[test]
    fn a_panic_on_another_thread_writes_no_report_into_this_tests_hook() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash-panic.log");
        let ring: &'static Ring = Box::leak(Box::new(Ring::new()));

        let restore = install_panic_hook_fenced_to_this_thread(
            Some(path.clone()),
            "scribobulate test\n".to_owned(),
            ring,
        );
        let other = std::thread::spawn(|| panic!("a concurrent test's deliberate panic"));
        let joined = other.join();
        restore();

        assert!(
            joined.is_err(),
            "precondition: the other thread really panicked"
        );
        assert!(
            !path.exists(),
            "another thread's panic reached this test's hook and wrote {}",
            path.display()
        );
    }

    #[test]
    fn the_panic_hook_writes_a_report_and_still_unwinds() {
        // TDD 21.7 both halves. Fenced to this thread, so a panic another test raises
        // in parallel cannot overwrite the report this one is about to read back.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash-panic.log");
        let ring: &'static Ring = Box::leak(Box::new(Ring::new()));
        ring.record("breadcrumb before the panic");

        let restore = install_panic_hook_fenced_to_this_thread(
            Some(path.clone()),
            "scribobulate test\n".to_owned(),
            ring,
        );
        let result = std::panic::catch_unwind(|| panic!("deliberate test panic"));
        restore();

        // Unwound (not aborted) — `catch_unwind` caught it.
        assert!(result.is_err());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("kind: panic"), "{text}");
        assert!(text.contains("deliberate test panic"), "{text}");
        assert!(text.contains("report.rs:"), "no panic location in:\n{text}");
        assert!(text.contains("breadcrumb before the panic"), "{text}");
    }

    /// The other half of TDD 21.7's new obligation: a panic `richimg` itself catches
    /// and converts to `Error::DecoderPanicked` must write NO crash report, and must
    /// still be logged so the failure is not silent.
    ///
    /// Drives a REAL guarded richimg panic — `richimg/tests/fixtures/crafted_182.webp`,
    /// the fixture richimg's own `webp_errors.rs` proves reliably panics
    /// `image-webp` 0.2.4 and is caught, so `contained_panic_in_progress()` is
    /// genuinely true while this hook runs, not merely asserted to be. The panic hook
    /// fires on EVERY panic regardless of `catch_unwind`, which is the property this
    /// test and the production hook both depend on.
    #[test]
    fn a_contained_decoder_panic_writes_no_crash_report_but_still_logs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash-panic.log");
        let ring: &'static Ring = Box::leak(Box::new(Ring::new()));

        let cap = crate::testlog::capture();
        let restore = install_panic_hook_fenced_to_this_thread(
            Some(path.clone()),
            "scribobulate test\n".to_owned(),
            ring,
        );

        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/richimg/tests/fixtures/crafted_182.webp"
        ));
        let limits = richimg::Limits::default();
        let result = richimg::first_frame(bytes, &limits);

        restore();

        assert_eq!(
            result,
            Err(richimg::Error::DecoderPanicked),
            "precondition: the fixture must still trigger the guarded panic this \
             test depends on — richimg/tests/webp_errors.rs proves it does"
        );
        assert!(
            !path.exists(),
            "a contained decoder panic must write NO crash report: {}",
            path.display()
        );
        assert!(
            cap.logged(log::Level::Error, "contained decoder panic"),
            "the contained panic must still be logged, or the failure is silent: {:?}",
            cap.records()
        );
    }
}
