//! The citation and document-path checks: 1, 2, 3, 6, 7 and 8.

use super::{fail, header, pass};
use crate::lint::patterns as rx;
use crate::lint::{Tree, CORPUS_FILE};

/// The register check 1 defends, and the plans it exempts. Named constants rather than
/// literals at the call site because a literal there reads to the gate's own check 1 as a
/// citation of the register by title — the gate is inside its own scan set now, which is
/// the point.
const ISSUES_REGISTER: &str = "sdd/ISSUES.md";
const PLAN_PREFIX: &str = "sdd/PLAN.";
use std::collections::BTreeSet;

/// Check 1 — no `sdd/ISSUES.md` entry referenced from outside that file.
///
/// SDD principle 6. ISSUES entries are ephemeral BY DESIGN — the register is working when
/// it empties — so every pointer to one is born with an expiry date and dangles the moment
/// the fix lands. Worse, if IDs are ever compacted it does not break loudly, it lies
/// quietly: the letter now names an unrelated issue. The cross-branch case is sharper still
/// and is what caught us: a reference written on a platform branch, where the issue is
/// real, dangles ON ARRIVAL when the change is cherry-picked to a tree that never had it.
///
/// Durable alternatives: cite an ANTI-PATTERNS entry (permanent by design), or write a
/// self-contained comment that explains the constraint and cites nothing.
///
/// `sdd/PLAN.*.md` is excluded: its dated entries are a historical record of what a thing
/// was called on a given day, not a live instruction.
pub fn issues_cited_outside_register(tree: &Tree) -> bool {
    header("1", "ISSUES referenced outside sdd/ISSUES.md");
    let findings = grep(
        tree,
        |path| path != ISSUES_REGISTER && !path.starts_with(PLAN_PREFIX) && path != CORPUS_FILE,
        |line| rx::issues_rx().is_match(line),
    );
    if findings.is_empty() {
        return pass();
    }
    fail(
        "replace with an ANTI-PATTERNS citation or a self-contained comment:",
        &findings,
        &[],
    )
}

/// Check 2 — every `ScrAP-N` cited in `src/` has a body in `sdd/ANTI-PATTERNS.md`.
///
/// Anti-patterns are permanent and ARE the correct citation target, so these references are
/// legitimate — but a number that names nothing is a dead end for the reader, and the
/// failure is silent. Expect this to fire during a cross-branch transfer: an entry authored
/// on a port branch is cited by code that reaches the shared branch first. That is a real
/// finding, not noise — the citing code and the cited entry need to land together, or in
/// that order.
pub fn scrap_cited_in_src_exists(tree: &Tree) -> bool {
    header(
        "2",
        "ScrAP-N cited in src/ but absent from sdd/ANTI-PATTERNS.md",
    );
    let defined = defined_numbers(tree);
    let mut findings = Vec::new();
    for number in cited_numbers(&tree.subset_texts(|path| path.starts_with("src/"))) {
        if defined.contains(&number) {
            continue;
        }
        findings.push(format!("ScrAP-{number} is cited but not defined:"));
        findings.extend(
            grep(
                tree,
                |path| path.starts_with("src/"),
                |line| line.contains(&format!("ScrAP-{number}")),
            )
            .into_iter()
            .map(|hit| format!("  {hit}")),
        );
    }
    if findings.is_empty() {
        return pass();
    }
    fail("cited numbers with no entry:", &findings, &[])
}

/// Check 3 — every `ScrAP-N` cited INSIDE the register has a body in it.
///
/// Check 2 scans `src/` against the register. It cannot see a register entry citing another
/// entry that is not there — and register-to-register is exactly where a cross-branch
/// transfer breaks, because a lesson authored on a port branch may cite a sibling that
/// stays on it. A transfer is also precisely when nobody is looking.
///
/// Deliberately matches only the `ScrAP-N` form, NOT the bare `#N` in-file shorthand: `#N`
/// cannot be told apart mechanically from ordinary prose (a sweep picks up `#287` and
/// `#819`, which are not citations at all). A check with false positives gets disabled; one
/// that is merely incomplete gets extended.
pub fn scrap_cited_in_register_exists(tree: &Tree) -> bool {
    header(
        "3",
        "ScrAP-N cited inside sdd/ANTI-PATTERNS.md but not defined there",
    );
    let register = tree.text("sdd/ANTI-PATTERNS.md").unwrap_or_default();
    let defined = defined_numbers(tree);
    let mut findings = Vec::new();
    for number in cited_numbers(&[("sdd/ANTI-PATTERNS.md", register)]) {
        if defined.contains(&number) {
            continue;
        }
        findings.push(format!(
            "ScrAP-{number} is cited in the register but has no `## {number}.` body:"
        ));
        for (index, line) in register.lines().enumerate() {
            if line.contains(&format!("ScrAP-{number}")) {
                findings.push(format!("  {}: {}", index + 1, truncate(line, 160)));
            }
        }
    }
    if findings.is_empty() {
        return pass();
    }
    fail("cited numbers with no entry:", &findings, &[])
}

/// Check 6 — every document path the tree points at exists.
///
/// The same failure class as check 1, arriving from the opposite direction: check 1 stops a
/// reference to an entry that is SUPPOSED to disappear, while this catches a reference left
/// behind when a whole FILE disappeared. Plan retirement is when it happens — a `PLAN.*.md`
/// is deleted by design once implemented, and every pointer written into the tree while it
/// existed dangles at once, scattered across code comments, `Cargo.toml` and the build
/// scripts.
///
/// TWO FORMS, because they dangle differently:
///
/// 6a — a directory-qualified path anywhere in the tree, OR a bare `PLAN.<topic>` citation
///      with or WITHOUT the `.md` extension, which needs no directory to be unambiguous:
///      plan files live in `sdd/` by convention and nowhere else, and the bare form is how
///      code comments overwhelmingly cite them. THE EXTENSION IS OPTIONAL BECAUSE THAT IS
///      HOW COMMENTS ACTUALLY CITE A PLAN — requiring `.md` was a blind spot that let 21
///      danglers survive a sweep that believed itself complete, since a comment cites a
///      SECTION (`PLAN.<topic> D3`) and a section citation names the plan, not its
///      filename.
///      `PLAN.md` alone is NOT a plan citation and is skipped: SDD names plans
///      `PLAN.<topic>.md`, so stripping the extension must leave a topic behind, and
///      without that rule `src/links.rs`'s link-parser fixture reads as a dangling plan.
///
/// 6b — a Markdown link target in a `.md` file, resolved relative to the linking file,
///      since that is how a reader's viewer resolves it.
///
/// WHAT IT DELIBERATELY DOES NOT MATCH — a bare document name in prose ("see POLICY.md for
/// the rule"), which is a mention, not a path: it resolves against nothing, and `src/`
/// alone carries ~100 of them. `tests/fixtures/` (link corpora broken ON PURPOSE) and
/// inline code spans are excluded from 6b for the same reason.
pub fn document_paths_resolve(tree: &Tree) -> bool {
    header("6", "referenced document paths that do not exist");
    let mut findings = Vec::new();

    // 6a. Directory-qualified paths and bare plan citations.
    for (name, text) in tree.subset_texts(|path| path != CORPUS_FILE) {
        for (index, line) in text.lines().enumerate() {
            for target in citation_targets(line) {
                if !tree.exists(&target) && !tree.exists(&format!("sdd/{target}")) {
                    findings.push(format!("{name}:{} -> {target}", index + 1));
                }
            }
        }
    }

    // 6b. Markdown link targets, resolved relative to the linking file.
    for (name, text) in
        tree.subset_texts(|path| path.ends_with(".md") && !path.starts_with("tests/fixtures/"))
    {
        let dir = name.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
        for line in text.lines() {
            // Code spans are stripped PER LINE, never over the whole file: a negated
            // character class matches a newline, so a whole-text strip pairs the backticks
            // of a fenced block with each other and silently swallows every link between
            // them. The unit is the line, which is also what a Markdown inline span is.
            let line = rx::code_span_rx().replace_all(line, "");
            for hit in rx::md_link_rx().captures_iter(&line) {
                let Some(target) = hit.get(1).map(|m| m.as_str()) else {
                    continue;
                };
                if target.contains(':')
                    || target.contains('~')
                    || target.starts_with("tests/reports/")
                {
                    continue;
                }
                let relative = if dir.is_empty() {
                    target.to_string()
                } else {
                    format!("{dir}/{target}")
                };
                if !tree.exists(&relative) && !tree.exists(target) {
                    findings.push(format!("{name} -> {target}"));
                }
            }
        }
    }

    findings.sort();
    findings.dedup();
    if findings.is_empty() {
        return pass();
    }
    fail(
        "these point at files that are not there:",
        &findings,
        &["Delete the pointer, or replace it with the fact it was carrying."],
    )
}

/// The document paths one line cites, as check 6a resolves them — the extraction AND its
/// skip rules in one function, so the corpus drives what the check does rather than a copy
/// of it. The PowerShell port's corpus asserted against its own re-implementation of this,
/// which is how a dropped `-CaseSensitive` stayed invisible to a green self-test.
///
/// Duplicates within a line are collapsed: one comment naming the same plan twice is one
/// finding.
pub fn citation_targets(line: &str) -> Vec<String> {
    let mut targets: Vec<String> = Vec::new();
    for hit in rx::path_rx().find_iter(line) {
        let matched = hit.as_str();
        // Target-side and ANCHORED: a link to a generated manual-test artefact is absent on
        // a fresh clone, so excluding the CONTAINING file instead would make the verdict
        // depend on what the developer had run locally — and an unanchored filter also
        // drops a legitimate link into a `reports/` directory that is NOT the generated
        // one, under `packaging/`.
        if matched.starts_with("tests/reports/") {
            continue;
        }
        // A bare `PLAN.<topic>` citation names a document, so give it the extension the
        // document has. `PLAN.md` strips to an empty topic and is not a plan citation.
        let target = match matched {
            "PLAN.md" => continue,
            plan if plan.starts_with("PLAN.") && !plan.ends_with(".md") => format!("{plan}.md"),
            other => other.to_string(),
        };
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    targets
}

/// The files that OPERATIONALLY declare the application ID. Deliberately not every file
/// that mentions it: `README.md`, the SVG, the manual-test plan and the port READMEs all
/// name it in prose, where a mention is not a declaration.
const APP_ID_FILES: [&str; 5] = [
    // The Linux install, which is where the ID is declared -- NOT the root `install.sh`,
    // which is a `uname -s` router carrying no install logic and therefore no ID to drift.
    // Listing the router instead would make this check pass on a file that cannot fail it.
    "packaging/linux/install.sh",
    "uninstall.sh",
    "data/scribobulate.desktop",
    "data/resources.gresource.xml",
    "packaging/macos/Info.plist.in",
];

/// Check 7 — the application ID does not drift between `src/icons.rs` and the packaging
/// files.
///
/// The ID is ONE string with several jobs — GApplication id, icon name, desktop-entry
/// `Icon=`/`StartupWMClass`, GResource path, macOS `CFBundleIdentifier`, the install target.
/// `src/icons.rs` is its source of truth and Rust derives it from there, so the Rust side
/// cannot drift; the non-Rust side is plain restated text and nothing checked it.
///
/// The failure is silent and platform-shaped: change the ID and the app keeps building and
/// running everywhere, while the installed desktop entry stops matching the window (no
/// taskbar icon on Linux), the GResource path stops resolving (the About logo becomes a
/// broken-image placeholder), and macOS Launch Services keeps registering the OLD
/// identifier. Each surface is checked by a different person on a different platform, or by
/// nobody.
pub fn app_id_drift(tree: &Tree) -> bool {
    header(
        "7",
        "app-ID drift between src/icons.rs and the packaging files",
    );
    let icons = tree.text("src/icons.rs").unwrap_or_default();
    let Some(canonical) = rx::app_id_rx()
        .captures(icons)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str())
    else {
        return fail(
            "the canonical app ID could not be read from src/icons.rs (Icon::App arm)",
            &[],
            &[],
        );
    };

    let mut findings = Vec::new();
    for file in APP_ID_FILES {
        let Some(text) = crate::lint::read_text(&tree.repo.join(file)) else {
            findings.push(format!("{file} is missing"));
            continue;
        };
        if !text.contains(canonical) {
            findings.push(format!("{file} does not carry the app ID '{canonical}'"));
        }
        // A DIFFERENT reverse-DNS id in the same file is drift the presence test above
        // cannot see — both can be true at once while one surface is stale.
        let foreign: BTreeSet<&str> = rx::reverse_dns_rx()
            .find_iter(&text)
            .map(|m| m.as_str())
            .filter(|found| *found != canonical)
            .collect();
        for found in foreign {
            findings.push(format!(
                "{file} carries a foreign app ID '{found}' (canonical: '{canonical}')"
            ));
        }
    }
    if findings.is_empty() {
        return pass();
    }
    fail(
        "the app ID is declared in more than one place and they disagree:",
        &findings,
        &["src/icons.rs's Icon::App arm is the source of truth; update the others."],
    )
}

/// Check 8 — a bare `AP-N` citation is illegal anywhere in this tree.
///
/// Not "means the skill" — illegal. Two registers are in play with unrelated numbering:
/// `sdd/ANTI-PATTERNS.md` (`ScrAP-N`) and the `gtk4-rs` skill (`GTK4Rs/AP-N`). A QA sample
/// put 83 of 107 entries at different numbers between them, and the collision is not
/// incidental — this register's #79 is the container-gesture lesson while the skill's is
/// the wall-clock-bounded pump loop, and its #88 and the skill's are the same pair
/// INVERTED. So a bare number is a coin toss between two real entries.
///
/// The bare form is the one that cannot be told apart from a missed sweep: it is
/// simultaneously the current skill spelling and the historical project spelling, so the
/// same string is correct or wrong depending on the day it was typed. A convention whose
/// correct and incorrect uses are textually identical is not a convention.
///
/// WHAT THIS DOES NOT DO, read before trusting a PASS: it cannot confirm a `GTK4Rs/AP-N`
/// names the right skill entry — the skill may not be installed, so nothing on this machine
/// can resolve it. That is the point rather than a hole. Making the bare form illegal does
/// not make skill citations VERIFIABLE, it makes them ENUMERABLE: the unverifiable set is
/// now exactly the `GTK4Rs/AP-N` citations, a greppable list a human can audit deliberately.
pub fn bare_ap_citations(tree: &Tree) -> bool {
    header("8", "bare AP-N citations (must be ScrAP-N or GTK4Rs/AP-N)");
    let mut findings = Vec::new();
    let mut citations = 0usize;
    for (name, text) in tree.subset_texts(|path| path != CORPUS_FILE) {
        for (index, line) in text.lines().enumerate() {
            let bare = rx::bare_ap_citations(line);
            if bare.is_empty() {
                continue;
            }
            citations += bare.len();
            // Report the CITATIONS, not the line. Register entries and manual-test items
            // run to well over a thousand characters, and a sweep this size is read as a
            // worklist — `path:line` followed by the citations themselves is actionable
            // where a truncated wall of
            // prose is not.
            findings.push(format!("{name}:{} — {}", index + 1, bare.join(" ")));
        }
    }
    if findings.is_empty() {
        return pass();
    }
    let headline = format!(
        "{citations} bare citation(s) on {} line(s):",
        findings.len()
    );
    fail(
        &headline,
        &findings,
        &[
            "Resolve each PER SITE: read the surrounding comment, decide which lesson it",
            "describes, then confirm that lesson's number IN THE REGISTER NAMED — the",
            "number must be RE-DERIVED, never carried across. If the lesson is in both",
            "registers, prefer ScrAP-N: this one is always resolvable, the skill may be",
            "absent. NEVER bulk-rewrite the prefix; that is the mechanism that produced",
            "the one confirmed defect.",
        ],
    )
}

/// `path:line:text` for every scan-set member the filter keeps whose line matches.
fn grep(
    tree: &Tree,
    keep: impl Fn(&str) -> bool + Copy,
    matches: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut hits = Vec::new();
    for (name, text) in tree.subset_texts(keep) {
        for (index, line) in text.lines().enumerate() {
            if matches(line) {
                hits.push(format!("{name}:{}:{line}", index + 1));
            }
        }
    }
    hits
}

/// The entry numbers `sdd/ANTI-PATTERNS.md` defines a body for.
fn defined_numbers(tree: &Tree) -> BTreeSet<u32> {
    tree.text("sdd/ANTI-PATTERNS.md")
        .unwrap_or_default()
        .lines()
        .filter_map(|line| rx::entry_number_rx().captures(line))
        .filter_map(|caps| caps.get(1)?.as_str().parse().ok())
        .collect()
}

/// The entry numbers cited as `ScrAP-N` across the given texts.
fn cited_numbers(texts: &[(&str, &str)]) -> BTreeSet<u32> {
    texts
        .iter()
        .flat_map(|(_, text)| rx::scrap_rx().captures_iter(text))
        .filter_map(|caps| caps.get(1)?.as_str().parse().ok())
        .collect()
}

fn truncate(line: &str, limit: usize) -> String {
    line.chars().take(limit).collect()
}
