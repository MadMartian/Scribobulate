//! The test-architecture checks (4, 5, 5b), the tracked-path legality check (12) and the
//! PowerShell encoding check (14).
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

/// Check 14 — every `.ps1` in the scan set carries a UTF-8 BOM or is pure ASCII.
///
/// Windows PowerShell 5.1 — the interpreter on a stock Windows box, and the one
/// `packaging/windows/pipeline.ps1` is run under — decodes a BOM-LESS script as the system
/// ANSI code page, not UTF-8. A UTF-8 em-dash in a comment then arrives as two mojibake
/// characters; the same bytes inside a STRING LITERAL or a regex reach the runtime as the
/// wrong characters, so the script does not merely look wrong, it behaves differently. The
/// file still parses, so nothing fails: this is a silent-divergence check like 4 and 5
/// above, not a style rule.
///
/// The invariant is BOM **or** pure ASCII, because either one removes the ambiguity: a BOM
/// tells 5.1 the encoding outright, and a file with no byte above 0x7F decodes identically
/// under every code page it could pick. A BOM is the stronger fix and the one to prefer —
/// it keeps non-ASCII safe anywhere in the file, literals included.
///
/// LINUX AND macOS CAN CHECK IT, which is why it lives here rather than on the Windows
/// seat: the hazard is a byte sequence, so the platform that cannot RUN the file is
/// perfectly able to read it. A check only Windows could run would be a check that fires
/// after the commit that broke it, on the one seat that is not authoring it.
pub fn powershell_encoding(tree: &Tree) -> bool {
    header(
        "14",
        "a .ps1 must carry a UTF-8 BOM or contain no byte above 0x7F",
    );
    let mut findings = Vec::new();
    for path in tree.scan.paths.iter().filter(|p| p.ends_with(".ps1")) {
        // BYTES, not `Tree::text`: the question is what the file's first three bytes are
        // and whether any byte is above 0x7F, and both facts are erased by a lossy decode.
        let Ok(bytes) = std::fs::read(tree.repo.join(path)) else {
            findings.push(format!("{path} is unreadable"));
            continue;
        };
        if let Some((at, byte)) = ps1_encoding_violation(&bytes) {
            findings.push(format!(
                "{path} — byte 0x{byte:02X} at offset {at} in a BOM-less file"
            ));
        }
    }
    if findings.is_empty() {
        return pass();
    }
    fail(
        "non-ASCII byte(s) in a BOM-less .ps1 (Windows PowerShell 5.1 reads these as ANSI):",
        &findings,
        &[
            "Add a UTF-8 BOM to the file, or keep it pure ASCII. The BOM is the stronger",
            "fix: it makes non-ASCII safe anywhere in the file, string literals included.",
        ],
    )
}

/// The predicate behind check 14: the offset and value of the first byte above 0x7F in a
/// file that carries no UTF-8 BOM, or `None` when the file is safe.
///
/// SEPARATE FROM THE CHECK so it can be exercised against the two fixtures under
/// `tests/fixtures/encoding/`, which are deliberately malformed and therefore excluded
/// from the scan set. A gate must be proven to FIRE, and the only way to prove this one
/// fires is to hand it bytes that must trip it.
pub fn ps1_encoding_violation(bytes: &[u8]) -> Option<(usize, u8)> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return None;
    }
    bytes
        .iter()
        .position(|byte| *byte > 0x7F)
        .map(|at| (at, bytes[at]))
}

/// Check 15 — a `match` over pulldown-cmark's vocabulary carries no UNANNOTATED
/// wildcard arm.
///
/// TDD 2.25 already requires the renderer's event dispatchers to match `Event`, `Tag`
/// and `TagEnd` EXHAUSTIVELY, so that a parser upgrade adding a construct fails to
/// compile rather than rendering it as nothing. The rule was sound and its MEMBERSHIP
/// was not: `copymap::classify` is a fourth dispatcher over the same vocabulary that
/// nobody had counted among the three, and it ended in `_ => return None`. Raw HTML
/// therefore put a `U+FFFC` in the buffer, earned no copy node, and silently omitted
/// its construct from copied source — for as long as `<picture>` had existed, with
/// every gate green.
///
/// So the check DISCOVERS dispatchers by the variants they name rather than reading a
/// list of them. A list would carry the identical hole: the next dispatcher written by
/// someone who did not know the list exists is exactly the case that produced this.
///
/// This is the compiler's own exhaustiveness enforcement, held open — the lint's only
/// job is to stop a wildcard being added back, since `Event` is not `#[non_exhaustive]`
/// and the compiler does the rest once no arm absorbs the remainder.
///
/// NOT EVERY WILDCARD FOUND THIS WAY IS A DISPATCH, though: a match can also SELECT
/// among already-classified events by something other than variant identity (`_ if
/// active =>`, gating on caller state a line above), and that shape is textually a
/// wildcard whether or not it behaves like one — pattern shape alone cannot tell the
/// two apart. Rather than guess from the arm's shape, the check accepts a wildcard that
/// carries `DISPATCH_SELECTOR_MARKER` (`dispatch-selector:`, `patterns.rs`) with a
/// non-empty reason, same line or an unbroken comment run directly above. A wildcard
/// with none is refused by DEFAULT — the marker is an opt-out that must be written, not
/// inferred, so a new dispatcher's wildcard fails until someone states why it is safe.
pub fn parser_dispatch_exhaustive(tree: &Tree) -> bool {
    header("15", "a pulldown-cmark dispatch swallows variants with `_`");
    let mut findings = Vec::new();
    for (name, text) in tree.subset_texts(|path| path.starts_with("src/") && path.ends_with(".rs"))
    {
        for line in rx::parser_dispatch_wildcards(text) {
            let body = text.lines().nth(line - 1).unwrap_or_default().trim();
            findings.push(format!("{name}:{line}: {body}"));
        }
    }
    if findings.is_empty() {
        return pass();
    }
    fail(
        "these matches absorb unnamed parser variants instead of naming them:",
        &findings,
        &[
            "Replace `_` with one arm per remaining variant. An arm that does nothing is",
            "still a DECISION — write it, with the reason. A construct that reaches the",
            "buffer without a deliberate arm is one that renders, or copies, as nothing.",
            "",
            "If the arm genuinely SELECTS on something other than which variant arrived",
            "(caller state, not Event/Tag/TagEnd identity), mark it instead of rewriting",
            "it: add `// dispatch-selector: <why this is a selection, not a dispatch>` on",
            "the arm's own line or directly above it. A bare marker with no reason does",
            "not count.",
        ],
    )
}
