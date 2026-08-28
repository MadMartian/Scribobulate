//! Test-only symlink setup, shared by every test whose subject is a symlink.
//!
//! # Why this is a module and not a helper in one test file
//!
//! The remedy for ScrAP-212 — create the link at test time, split by target family,
//! and SKIP LOUDLY where the platform refuses — was written once, inside
//! `links.rs`'s own `mod tests`, where nothing else could reach it. Three other
//! tests over the same subject kept the shape it replaced (`#[cfg(unix)]`), so the
//! lesson was applied at two sites out of five and the remaining three stayed
//! deleted-on-Windows. A remedy that lives inside one consumer is a remedy the next
//! consumer will not find; hoisting it here is what makes "use the runtime skip"
//! the path of least resistance rather than a thing to remember.
//!
//! # The rule this encodes
//!
//! `#[cfg(unix)]` on a test does not skip it on Windows — it **deletes** it. It is
//! not compiled, not reported, and not counted, and no test harness has a column in
//! which "never built" differs from "passed". The suite is green over a limb it
//! never compiled. So the platform *exclusion* is replaced by a platform-appropriate
//! *implementation* plus an explicit runtime skip that can be printed, counted and
//! grepped — `packaging/windows/pipeline.ps1` greps `SKIPPED \[` and reports how
//! many tests passed without verifying their subject.
//!
//! Creating a symlink on Windows needs Developer Mode or an elevated shell, so an
//! ordinary developer box legitimately cannot run these; that is a skip and it is
//! reported as one. On unix there is no such excuse, so a failure to create is a
//! real failure and is left to panic.
//!
//! # A skip is a last resort, not the Windows answer
//!
//! [`escaping_reference_or_skip`] exists because the *containment* limb — "a link
//! inside the directory cannot reach outside it" — does not actually need a symlink,
//! it needs a **reparse point**, and Windows has one an unprivileged process may
//! create: an NTFS **directory junction** (`mklink /J`). MEASURED on an ordinary,
//! unelevated box with Developer Mode off: `symlink_file` fails with os error 1314
//! ("A required privilege is not held by the client") while `mklink /J` returns 0,
//! and `std::fs::canonicalize` follows the junction to its target outside the
//! directory — which is the whole mechanism the containment gate is checked against.
//!
//! That matters disproportionately here because **Windows owns row 1 of the sprite
//! search path** (`%APPDATA%`, `sdd/SCHEMA.md`), so the platform whose user-writable
//! theme directory is most exposed was the one reporting the limb unverified. The
//! skip is still reachable and still printed when *both* mechanisms refuse; what
//! changed is that it is no longer the first answer.
//!
//! Not every symlink test can be served this way and none should be forced to: a
//! junction is a **directory**, so a check whose subject is the link's own *name*
//! (the sprite extension allowlist, `chip.png → notes.txt`) has no junction form and
//! correctly still skips here.

use std::path::Path;

/// Report that `limb` went **unverified** on this host, in the form the packaging
/// pipeline greps for.
///
/// The general primitive behind [`symlink_or_skip`], hoisted for the reason the module
/// header gives: the remedy for a platform-excluded test is a runtime skip that can be
/// printed, counted and grepped, and a remedy only reachable from the symlink helper is
/// one the next `#[cfg(unix)]`-shaped test will not find. The crash-report and
/// seen-marker permission checks (TDD 21.12) are that next test — "owner-only" is a
/// mode question on unix and an ACL question on Windows, so the assertion is genuinely
/// unix-shaped, but the *rubric* still has to be reported as unverified rather than
/// vanish from the run.
///
/// `limb` is the rubric, not the test's name — same rule as [`symlink_or_skip`].
///
/// # One write, deliberately — not `eprintln!`
///
/// The pipeline runs `cargo test -- --nocapture`, under which libtest interleaves its
/// own `test <name> ... ` / `ok` progress writes with anything a test prints. A
/// *formatted* `eprintln!` reaches stderr as one write **per format fragment**, so
/// another thread's `ok` can land between `SKIPPED [rubric]: ` and the reason.
/// MEASURED on the real command, twice in four runs; one line came out
///
/// ```text
/// test copymap::tests::within_link_caption_excludes_brackets_and_url ... SKIPPED [TDD 24.13 stored spelling]: ok
/// ```
///
/// — the announcement that a rubric went **unverified**, rendered as a pass, in the
/// one mechanism the project has for saying otherwise (ScrAP-273). Building the line
/// first and emitting it with a single `write_all` closes it: a sub-`PIPE_BUF` write
/// is atomic on the pipe the pipeline reads through. A literal-only `eprintln!` is
/// already one write and was never affected, which is why this went unnoticed — every
/// site that had a reason worth interpolating was the vulnerable kind.
pub(crate) fn skipped(limb: &str, why: &str) {
    use std::io::Write;
    let line = format!(
        "SKIPPED [{limb}]: {why}. The behaviour this test verifies is NOT verified by \
         this run.\n"
    );
    let _ = std::io::stderr().lock().write_all(line.as_bytes());
}

/// Create a file symlink, or return why the platform refused.
///
/// Split by target family rather than `#[cfg(unix)]`-ing the caller away, because
/// those two are not the same thing — see the module header.
fn try_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(target, link)
    }
}

/// `Ok(())` when the symlink was created and is genuinely a symlink; `Err(())` when
/// this platform would not allow it, having printed a `SKIPPED [...]` line naming the
/// real OS error.
///
/// `limb` is the **rubric** the caller is verifying (e.g. `"TDD 19.2 symlink"`), not
/// the test's name: the label is what a reader of the pipeline's skip report needs in
/// order to know which guarantee went unverified, and renaming a test must not
/// silently retarget it.
///
/// On unix the refusal path panics rather than returning — a unix box that cannot
/// create a symlink in a temp dir has a real problem, and quietly skipping there
/// would reintroduce the invisible absence this whole mechanism exists to prevent.
///
/// The caller is expected to `return` on `Err`:
///
/// ```ignore
/// if symlink_or_skip(&target, &link, "TDD 19.2 symlink").is_err() {
///     return;
/// }
/// ```
pub(crate) fn symlink_or_skip(target: &Path, link: &Path, limb: &str) -> Result<(), ()> {
    match symlink_checked(target, link) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Through `skipped`, so the pipeline greps ONE format. Two sites
            // formatting the same marker independently is how a report ends up
            // counting some skips and not others.
            skipped(
                limb,
                &format!(
                    "cannot create a symlink on this host ({e}). Windows requires \
                     Developer Mode or an elevated shell"
                ),
            );
            Err(())
        }
    }
}

/// Create the symlink and prove it is one, WITHOUT announcing a skip.
///
/// Split out from [`symlink_or_skip`] when [`escaping_reference_or_skip`] gained a
/// second mechanism, and the split is the load-bearing part rather than a tidy-up: a
/// caller that falls back to a junction must not have already printed
/// `SKIPPED [...]: ... NOT verified by this run` for an attempt it then went on to
/// make succeed by other means. The pipeline greps that marker and counts the limbs it
/// names, so a skip line emitted before a successful fallback would report the
/// guarantee as unverified in the one mechanism the project has for saying so —
/// ScrAP-273's shape, arrived at from the other direction.
fn symlink_checked(target: &Path, link: &Path) -> std::io::Result<()> {
    if let Err(e) = try_symlink(target, link) {
        if cfg!(unix) {
            panic!("symlink creation failed on a unix host, which should not happen: {e}");
        }
        return Err(e);
    }
    // Prove the SETUP before trusting the verdict: a test whose fixture is not what
    // it claims reports on something else entirely, and passes while doing it
    // (ScrAP-209). On Windows in particular a "successful" creation that left a plain
    // file behind would make every assertion after it meaningless — which is exactly
    // how the checked-in `core.symlinks=false` fixture came to exercise the INVERSE
    // of its intent while looking like an unremarkable pass.
    let meta = std::fs::symlink_metadata(link).expect("the link we just created must exist");
    assert!(
        meta.file_type().is_symlink(),
        "setup produced a {:?}, not a symlink -- the assertion that follows would \
         be testing an ordinary file and would pass for the wrong reason",
        meta.file_type()
    );
    Ok(())
}

/// Create an NTFS **directory junction** at `link` pointing at the directory
/// `target_dir`, and prove it reads through.
///
/// Shelling out to `mklink` is deliberate and is not laziness: creating a junction is
/// a `DeviceIoControl(FSCTL_SET_REPARSE_POINT)` call with a hand-built
/// `REPARSE_DATA_BUFFER`, `std` exposes no API for it, and this is test-only setup —
/// the project's Win32 FFI rule (POLICY § architecture) is about *production* calls
/// past GTK, and hand-rolling a reparse buffer here would put a second, unreviewed
/// Win32 surface in the tree to save a process spawn in a test.
///
/// `.output()` rather than `.status()` so `mklink`'s success chatter stays out of the
/// harness stream, where it would interleave with libtest's own progress writes for
/// the same reason [`skipped`] builds its line before emitting it.
#[cfg(windows)]
fn try_junction(target_dir: &Path, link: &Path) -> std::io::Result<()> {
    use std::io::Error;

    let out = std::process::Command::new("cmd")
        .arg("/c")
        .arg("mklink")
        .arg("/J")
        .arg(link)
        .arg(target_dir)
        .output()?;
    if !out.status.success() {
        return Err(Error::other(format!(
            "mklink /J failed ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

/// A theme-relative reference that leaves `inside_dir` by way of a link, or `Err(())`
/// having printed a `SKIPPED [...]` line naming **every** mechanism this host refused.
///
/// The caller gets a *reference string* rather than a path because the two mechanisms
/// produce different shapes and the difference is load-bearing: a file symlink is
/// `link.png` and a directory junction is `link/<name>`, and a caller that assumed the
/// first would silently stop traversing anything on the host that used the second.
///
/// # What it proves before it returns
///
/// The fixture, never the verdict. `symlink_or_skip` already asserts a symlink is a
/// symlink (ScrAP-209's species — a fixture that is not what it claims reports on
/// something else and passes while doing it); a junction has to clear the same bar,
/// and "mklink printed success" does not clear it. So the junction arm canonicalises
/// the reference and asserts it lands on the **outside** file — the link genuinely
/// reads through — before the caller is allowed to assert that `resolve` refuses it.
/// Without that, a junction that silently resolved to nothing would make the
/// containment assertion pass for the wrong reason, which is the exact failure this
/// module exists to prevent.
pub(crate) fn escaping_reference_or_skip(
    inside_dir: &Path,
    outside_file: &Path,
    limb: &str,
) -> Result<String, ()> {
    let name = outside_file
        .file_name()
        .expect("the escape target must be a file, not a directory");
    let outside_dir = outside_file
        .parent()
        .expect("the escape target must live in a directory");

    // Preferred everywhere, and the only mechanism on unix -- where a failure is a
    // broken host and `symlink_checked` panics rather than returning.
    let link = inside_dir.join(name);
    let refused = match symlink_checked(outside_file, &link) {
        Ok(()) => return Ok(name.to_string_lossy().into_owned()),
        Err(e) => e,
    };

    #[cfg(windows)]
    {
        let junction = inside_dir.join("junction");
        if let Err(e) = try_junction(outside_dir, &junction) {
            // BOTH mechanisms named, because a reader of the skip report has to know
            // what was actually tried on this host -- a line saying only "symlink"
            // where a junction was also refused misdescribes the box it ran on.
            skipped(
                limb,
                &format!(
                    "this host allows neither a file symlink ({refused}; Windows \
                     requires Developer Mode or an elevated shell) nor an NTFS \
                     directory junction ({e})"
                ),
            );
            return Err(());
        }
        // Prove the fixture, per the doc comment above: the junction must READ
        // THROUGH to the file outside, or the containment assertion behind it would
        // be about a path that resolves to nothing.
        let through = junction.join(name);
        let landed = through.canonicalize().unwrap_or_else(|e| {
            panic!("the junction we just created does not read through to {through:?}: {e}")
        });
        let want = outside_file
            .canonicalize()
            .expect("the escape target must exist before the fixture is built");
        assert_eq!(
            landed, want,
            "the junction resolves somewhere other than its target -- the containment \
             assertion behind this would pass for the wrong reason"
        );
        Ok(format!("junction/{}", name.to_string_lossy()))
    }

    // Unreachable on unix: `symlink_checked` panics there rather than returning `Err`,
    // which is the asymmetry the module header states -- a unix box that cannot make a
    // symlink in a temp directory has a real problem and must not be let past as a skip.
    #[cfg(not(windows))]
    {
        // `limb` too: it is read only by the junction arm's `skipped` call, and this
        // file is compiled on every platform, so an unused-variable error here is the
        // cross-platform asymmetry POLICY warns about arriving in the gate itself.
        let _ = (outside_dir, refused, limb);
        unreachable!("a unix host that refuses a symlink has already panicked")
    }
}
