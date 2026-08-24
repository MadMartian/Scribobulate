//! The test-architecture checks (4, 5, 5b) and the tracked-path legality check (12).
//!
//! The first three share a failure mode rather than a subject: each guards something that
//! breaks SILENTLY, where the tree still builds, every test still passes, and the only
//! symptom is a test body that quietly stopped running.

use super::{fail, header, pass};
use crate::lint::contract::illegal_tracked_paths;
use crate::lint::patterns as rx;
use crate::lint::Tree;
use std::collections::BTreeSet;

/// Check 4 — the module list in `src/lib.rs` and `src/gtk_suite.rs` has not drifted.
///
/// `src/gtk_suite.rs` is a SECOND CRATE ROOT (a `harness = false` target that runs the GTK
/// bodies on the process main thread). It has to re-declare `src/lib.rs`'s module list, and
/// that duplication fails SILENTLY: a new top-level module added to `lib.rs` and not to the
/// suite root simply drops every `#[gtktest::test]` body inside it from the suite. Nothing
/// errors, no test fails, and the only visible symptom is a case count nobody has memorised
/// — on the platform where the suite is the ONLY way those bodies run at all.
///
/// Only plain `mod x;` lines are compared, which is what makes `suite_registry` (cfg-gated
/// in the lib, unconditional in the suite root) and the `#[cfg(unix)]`/`#[cfg(windows)]`
/// modules compare equal rather than reading as drift.
pub fn module_list_drift(tree: &Tree) -> bool {
    header(
        "4",
        "module list drift between src/lib.rs and src/gtk_suite.rs",
    );
    let lib = modules(tree, "src/lib.rs");
    let suite = modules(tree, "src/gtk_suite.rs");
    let only_lib: Vec<&String> = lib.difference(&suite).collect();
    let only_suite: Vec<&String> = suite.difference(&lib).collect();

    let mut ok = true;
    if !only_lib.is_empty() {
        ok = fail(
            "declared in src/lib.rs but MISSING from src/gtk_suite.rs:",
            &only_lib
                .iter()
                .map(|name| (*name).clone())
                .collect::<Vec<_>>(),
            &["Every #[gtktest::test] in these modules is silently absent from the suite."],
        );
    }
    if !only_suite.is_empty() {
        ok = fail(
            "declared in src/gtk_suite.rs but not in src/lib.rs:",
            &only_suite
                .iter()
                .map(|name| (*name).clone())
                .collect::<Vec<_>>(),
            &[],
        );
    }
    if ok {
        return pass();
    }
    false
}

/// Check 5 — `#[gtk::test]` has not come back in `src/`.
///
/// The attribute still exists and still works — it is simply the wrong choice here, and
/// choosing it is INVISIBLE: the test passes on Linux, so nothing fails, while the body is
/// absent from `src/gtk_suite.rs`'s main-thread run and therefore from the only run
/// available on a platform where GTK must initialise on the main thread.
///
/// The enforcement ladder is convention → lint → compiler. A clippy `disallowed-methods`
/// ban cannot reach an attribute macro and the compiler cannot be made to care, so a lint
/// is the strongest rung available.
///
/// Deliberately NOT widened past `src/*.rs`: `tests/` and `gtktest/` name the banned
/// attribute in doc comments by necessity (gtktest's whole purpose is to replace it), and a
/// check with false positives gets disabled while an incomplete one gets extended.
pub fn legacy_gtk_test_attribute(tree: &Tree) -> bool {
    header("5", "#[gtk::test] used instead of #[gtktest::test]");
    let mut findings = Vec::new();
    for (name, text) in tree.subset_texts(|path| path.starts_with("src/") && path.ends_with(".rs"))
    {
        for (index, line) in text.lines().enumerate() {
            if line.trim() == "#[gtk::test]" {
                findings.push(format!("{name}:{}:{line}", index + 1));
            }
        }
    }
    if findings.is_empty() {
        return pass();
    }
    fail(
        "these bodies would be absent from the main-thread suite:",
        &findings,
        &["Replace with #[gtktest::test] (drop-in; no other change needed)."],
    )
}

/// Check 5b — the same attribute, PRESCRIBED IN PROSE.
///
/// RATIFIED BY THE OPERATOR (2026-07-31). Check 5 greps `src/*.rs`, so it can only ever see
/// a developer who has already written the wrong thing — it cannot see the DOCUMENT that
/// told them to. QA round 4 found `sdd/POLICY.md` instructing `#[gtk::test]` in its testing
/// section while check 5 rejected that exact attribute: two artifacts, each internally
/// correct, enforcing opposite things, and neither able to detect the other. A reader
/// obeying the written rule was punished by the automated one (ScrAP-222).
///
/// THE DISCRIMINATOR, and the reason this is not a blanket grep: these documents MUST be
/// able to name the banned attribute — the paragraph explaining why it is banned says it
/// three times, and a gate that failed on that would be unable to tell a MENTION from a
/// USE, and so guaranteed to be disabled. A legitimate mention always contrasts the two
/// attributes; a stale PRESCRIPTION stands alone.
///
/// THE INPUT SET IS HALF THIS GATE (ScrAP-207): only documents that INSTRUCT are scanned,
/// and the set comes from the contract's `prescriptive` class, not a literal here.
/// `sdd/ANTI-PATTERNS.md` and the review reports are excluded on purpose — the register
/// DESCRIBES a past state and a report QUOTES the defect verbatim, which is the point of a
/// report. Add a document to the class only if a reader could reasonably ACT on its wording.
pub fn legacy_attribute_prescribed(tree: &Tree) -> bool {
    header(
        "5b",
        "#[gtk::test] PRESCRIBED in prose without naming the replacement",
    );
    let documents: Vec<(String, String)> = tree
        .scan
        .prescriptive
        .iter()
        .map(|path| {
            (
                path.clone(),
                tree.text(path).unwrap_or_default().to_string(),
            )
        })
        .collect();
    let findings: Vec<String> = rx::prose_prescriptions(&documents)
        .into_iter()
        .map(|hit| format!("{}:{}: {}", hit.file, hit.line, hit.text))
        .collect();
    if findings.is_empty() {
        return pass();
    }
    fail(
        "prose prescribing the attribute check 5 rejects:",
        &findings,
        &[
            "A reader who obeys this breaks the build. Name #[gtktest::test] in the",
            "same paragraph, so the passage reads as a contrast rather than an",
            "instruction.",
        ],
    )
}

/// Check 12 — no tracked path is one Windows refuses to check out.
///
/// `< > : " | ? *`, a trailing dot or space, a control character, or a reserved device name
/// makes `git checkout` refuse the WHOLE tree — "error: invalid path", nothing applied.
/// Every Windows clone, and any Windows user landing on or bisecting through the commit, is
/// blocked.
///
/// MEASURED, and it is why this check exists rather than being a theoretical nicety: an
/// unquoted `sed -i 's|a|b|'` had its `|` eaten by the shell, wrote the replacement half to
/// disk AS A FILENAME, and `git add -A` committed it. Nothing on Linux or macOS noticed —
/// not fmt, not clippy, not the suite, not the other checks here. Only the Windows seat
/// could see it, one fetch later, and only by being blocked.
///
/// INPUT SET IS `git ls-files`, DELIBERATELY — not the scan contract. The scan is a curated
/// list and that file landed in the repo ROOT, outside it; a check whose input set is
/// narrower than its hazard is ScrAP-132's species.
///
/// PLANTING A TEST CASE IS PLATFORM-SPECIFIC, and the platform this check defends fights
/// hardest against arming it. On Linux/macOS: `touch 'bad|name.txt' && git add`. On Windows
/// the working-tree file cannot be created at all, and even `git update-index` refuses it —
/// the plant needs the guard off:
///
/// ```text
/// git -c core.protectNTFS=false update-index --add --cacheinfo "100644,<sha>,bad|name.txt"
/// git -c core.protectNTFS=false update-index --force-remove "bad|name.txt"
/// ```
///
/// REMOVE PLANTS BY EXPLICIT NAME. Do not pipe this check's output into a `git rm --cached`
/// loop: a planted path containing a TAB is re-split by the reader, the loop then hands git
/// a pathspec nobody intended, and it staged the deletion of 341 tracked files here before
/// `git checkout -- .` put them back. The cleanup for a check about dangerous filenames
/// must not itself be filename-driven.
pub fn windows_illegal_paths(tree: &Tree) -> bool {
    header("12", "paths Windows cannot check out");
    match illegal_tracked_paths(&tree.repo) {
        Err(why) => fail(&why, &[], &[]),
        Ok(bad) if bad.is_empty() => pass(),
        Ok(bad) => fail(
            "tracked path(s) illegal on Win32 (git checkout refuses the whole tree):",
            &bad.iter()
                .map(|path| format!("{path:?}"))
                .collect::<Vec<_>>(),
            &[],
        ),
    }
}

/// The plain `mod x;` names a crate root declares.
fn modules(tree: &Tree, path: &str) -> BTreeSet<String> {
    tree.text(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| rx::mod_decl_rx().captures(line))
        .filter_map(|caps| Some(caps.get(2)?.as_str().to_string()))
        .collect()
}
