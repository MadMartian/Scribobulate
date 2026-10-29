//! The citation and document-path checks: 1, 2, 3, 6, 7, 8, 20, 25 and 26.

use super::{fail, header, pass};
use crate::lint::patterns as rx;
use crate::lint::{Tree, CORPUS_FILE};

/// Self-exclusion: this check's own pattern file spells hex runs out as test-visible
/// literals, so it would report itself. The same carve-out `CORPUS_FILE` already has.
const PATTERNS_FILE: &str = "xtask/src/lint/patterns.rs";

/// The register check 1 defends, and the plans it exempts. Named constants rather than
/// literals at the call site because a literal there reads to the gate's own check 1 as a
/// citation of the register by title — the gate is inside its own scan set now, which is
/// the point.
const ISSUES_REGISTER: &str = "sdd/ISSUES.md";
const PLAN_PREFIX: &str = "sdd/PLAN.";
/// The security register check 26 resolves `ScrSEC-N` against.
const CVE_REGISTER: &str = "sdd/CVE.md";
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
    // The Linux PAYLOAD and the uninstaller, which is where the ID is declared. NOT
    // `packaging/linux/install.sh`: it holds no layout of its own, and the ID with it,
    // since all three Linux routes (this install, the .deb and the .rpm) read
    // `payload.sh` for what goes where. NOT the root `install.sh`/`uninstall.sh` either
    // -- those are `uname -s` routers carrying no install logic and so no ID to drift;
    // listing a file that cannot fail this check reads as coverage and is not. The macOS
    // scripts are absent for the same reason: they address the bundle by path.
    "packaging/linux/payload.sh",
    "packaging/linux/uninstall.sh",
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
        // A WHOLE-identifier test, not a substring one: `contains` is satisfied by any
        // superset of the canonical ID, so a file declaring `…scribobulated` would report
        // as carrying `…scribobulate` (see `declares_whole_id`).
        if !rx::declares_whole_id(&text, canonical) {
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

/// Check 20 — no tracked file cites a git commit hash.
///
/// POLICY § SDD register writes: this project squashes each batch, so a hash names a
/// commit designed to stop existing. The failure is not loud — an orphaned hash still
/// resolves in the clone that wrote it, which is the clone anyone verifying it is
/// standing in, so the citation looks sound exactly where it is checked and is a dead
/// end everywhere else.
///
/// **This rustdoc previously asserted that the check had found the eight citations
/// review reported, "two of them in the permanent register". It had not.** The first
/// version exempted a line if it contained a marker ANYWHERE, and the marker list held a
/// bare lowercase `gtk`; in a GTK project that is close to a blanket exemption, and both
/// register citations were green under it. A gate passing over its own motivating
/// instances is bad; a gate whose documentation claims it caught them is worse, because
/// the next reader concludes the class is handled. The exemption is positional now — see
/// `rx::attributed_nearby` — and the corpus carries both of those real lines as cases
/// that must flag.
///
/// **The carve-outs are in the predicate, and the order was the point.** POLICY exempts
/// the generated crash-report build stamp; a lint written before that exemption was
/// restored to POLICY would flag it, and an agent enforcing the surviving flat rule
/// deletes the crash reporter's only link back to its own source revision. See
/// `rx::commit_hash_citations`.
///
/// `sdd/PLAN.*.md` is NOT excluded: a plan is read at the start of the work it describes,
/// so a dead hash there is read by the one person least able to resolve it.
pub fn commit_hashes_cited(tree: &Tree) -> bool {
    header(
        "20",
        "git commit hashes cited (POLICY: cite the subject and date)",
    );
    let findings = grep(
        tree,
        |path| path != CORPUS_FILE && path != PATTERNS_FILE,
        |line| !rx::commit_hash_citations(line).is_empty(),
    );
    if findings.is_empty() {
        return pass();
    }
    fail(
        "a squashed hash resolves only in the clone that wrote it:",
        &findings,
        &[
            "cite the fact, a register entry (ScrAP-N / GTK4Rs/AP-N / GEP-N / TDD SS N),",
            "or the commit SUBJECT plus its date.",
        ],
    )
}

/// Check 25 — no file cites a numbered POLICY step.
///
/// POLICY § Build pipeline numbers none of its steps; the numbered list became a pointer
/// to `scripts/pipeline.steps`, and every "POLICY step N" written against the old list
/// went on naming an address that no longer exists — including a non-applicable reason
/// the Linux run prints. Check 6 resolves PATHS, so a section-plus-step citation was
/// invisible to it. Cite the section, plus the rule's name where it helps:
/// "POLICY § Build pipeline (coverage ratchet)". A step NUMBER belongs to the contract
/// and is cited from it ("build-pipeline step 5b"), never from POLICY.
///
/// WHAT THIS DOES NOT DO: it cannot tell whether a section citation names the RIGHT
/// section — the same limit check 2 states for entry numbers. See
/// `rx::policy_step_citations` for the cross-line reading. Corpus: `corpus_policystep.rs`.
pub fn policy_step_cited(tree: &Tree) -> bool {
    header(
        "25",
        "a citation of a numbered POLICY step (POLICY numbers none)",
    );
    let mut findings = Vec::new();
    for (name, text) in tree.subset_texts(|path| path != CORPUS_FILE && path != PATTERNS_FILE) {
        let lines: Vec<&str> = text.lines().collect();
        for number in rx::policy_step_citations(text) {
            let line = lines.get(number - 1).copied().unwrap_or_default();
            findings.push(format!("{name}:{number}:{line}"));
        }
    }
    if findings.is_empty() {
        return pass();
    }
    fail(
        "POLICY no longer numbers its pipeline steps:",
        &findings,
        &[
            "cite the section instead: POLICY § Build pipeline, with the rule's name",
            "where it helps, e.g. (coverage ratchet). Step numbers are the contract's",
            "(scripts/pipeline.steps), and are cited from it, never from POLICY.",
        ],
    )
}

/// Check 26 — every `ScrSEC-N` citation is well-formed and names an entry `sdd/CVE.md`
/// defines, and every entry there has a Contents row (and every row an entry).
///
/// The security register's entries are permanent like ANTI-PATTERNS', so they are the
/// citable home for a workaround's history — `src/imagedecode/xinclude.rs` cites one. A
/// number that names nothing sends the next reader changing that workaround to no record
/// of the leaks already found in it, which is the one read the citation exists to force.
///
/// Scans the whole tree, not `src/` alone as check 2 does for `ScrAP-N`: the citations
/// today sit in ISSUES, a plan and POLICY as much as in code.
pub fn scrsec_citations_resolve(tree: &Tree) -> bool {
    header(
        "26",
        "ScrSEC-N citations malformed, or absent from sdd/CVE.md or its Contents",
    );
    let register = tree.text(CVE_REGISTER).unwrap_or_default();
    let numbers = |pattern: &regex::Regex| -> BTreeSet<u32> {
        register
            .lines()
            .filter_map(|line| pattern.captures(line))
            .filter_map(|caps| caps.get(1)?.as_str().parse().ok())
            .collect()
    };
    let defined = numbers(rx::scrsec_heading_rx());
    let listed = numbers(rx::scrsec_toc_row_rx());
    let mut findings = Vec::new();
    for (name, text) in tree.subset_texts(|path| path != CORPUS_FILE) {
        for (index, line) in text.lines().enumerate() {
            for caps in rx::scrsec_rx().captures_iter(line) {
                let Some(number) = caps.get(1).and_then(|m| m.as_str().parse::<u32>().ok()) else {
                    continue;
                };
                if !defined.contains(&number) {
                    findings.push(format!(
                        "{name}:{} — ScrSEC-{number} has no `## ScrSEC-{number}.` entry",
                        index + 1
                    ));
                }
            }
            for form in rx::malformed_scrsec_citations(line) {
                findings.push(format!(
                    "{name}:{} — `{form}` is not the citation form ScrSEC-N",
                    index + 1
                ));
            }
        }
    }
    for number in defined.difference(&listed) {
        findings.push(format!(
            "{CVE_REGISTER}: ScrSEC-{number} has an entry but no Contents row"
        ));
    }
    for number in listed.difference(&defined) {
        findings.push(format!(
            "{CVE_REGISTER}: ScrSEC-{number} has a Contents row but no entry"
        ));
    }
    if findings.is_empty() {
        return pass();
    }
    fail(
        "security-register citations that resolve to nothing:",
        &findings,
        &["Write `ScrSEC-<digits>` exactly; add the entry (and its Contents row) first."],
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
