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
pub(crate) fn skipped(limb: &str, why: &str) {
    eprintln!(
        "SKIPPED [{limb}]: {why}. The behaviour this test verifies is NOT verified by \
         this run."
    );
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
    match try_symlink(target, link) {
        Ok(()) => {}
        Err(e) => {
            if cfg!(unix) {
                panic!("symlink creation failed on a unix host, which should not happen: {e}");
            }
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
            return Err(());
        }
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
