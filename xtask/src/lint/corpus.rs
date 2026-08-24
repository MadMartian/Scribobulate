//! The corpora — the gate on the gate.
//!
//! A GATE MUST BE PROVEN TO FIRE. The first version of check 1 shipped with a pattern that
//! missed three real citation forms, including the exact form of one of the two defects
//! that motivated writing the gate; it was found by mutation-testing rather than by
//! trusting a clean PASS. So each pattern here has a positive corpus (it must match) and a
//! negative one (it must not), and every case calls the predicate THE CHECK CALLS. A corpus
//! over a re-implementation proves the copy — which is how a dropped case-sensitivity flag
//! stayed invisible to a green self-test on the port this crate replaced.
//!
//! THIS IS THE ONE FILE THE GATE EXCLUDES FROM ITS OWN TEXT CHECKS (`lint::CORPUS_FILE`),
//! because the corpora below quote the very citation forms checks 1, 6a and 8 hunt for,
//! exactly as a linter's own fixtures do. One path, not the pair the two shell ports had to
//! keep in step by hand.
//!
//! These are ordinary `#[test]` cases: build-pipeline step 4 runs them, and step 2 lints
//! them. That is the whole reason this gate is a Cargo crate — its predecessor's corpus
//! runner was hand-rolled, ran only when someone remembered to pass `--self-test`, and was
//! where that round's defects concentrated.

use crate::lint::checks::references::citation_targets;
use crate::lint::contract::{git_index, Contract, ScanSet};
use crate::lint::patterns::{bare_ap_citations, issues_rx, prose_prescriptions, win_illegal_path};
use std::path::{Path, PathBuf};

// ── Check 1: the ISSUES citation forms ────────────────────────────────────────

const ISSUES_MUST_MATCH: &[&str] = &[
    "see ISSUES P",
    "see sdd/ISSUES.md entry P for details",
    "see ISSUES.md entry P",
    "see ISSUES-P",
    "see ISSUES_P",
    "see ISSUES: P",
    "(TDD 17.31 / ISSUES #BBB)",
    "regression, see ISSUES #P",
    "see sdd/ISSUES.md #WW",
    "(ISSUES.md item P)",
    "see ISSUES issue P",
    "the ISSUES.md letter P",
    // The TITLE form. A pointer by quoted title dangles exactly as an ID pointer does —
    // the entry is deleted when fixed either way — and a pattern hunting letter IDs
    // reported PASS over a live one.
    "attribute the growth (ISSUES.md \"native file chooser RSS growth\")",
    "see ISSUES \"the rename suite finalizes a cancelled monitor\"",
];

/// Prose ABOUT the register, which must stay legal: the rule is against citing an ENTRY,
/// not against naming the file. A check with false positives gets disabled.
const ISSUES_MUST_NOT_MATCH: &[&str] = &[
    "issues a queue_draw that CLEARS the cache",
    "see sdd/ISSUES.md",
    "the sdd/ISSUES.md TOC lists them",
    "the sdd/ISSUES.md IDs are ephemeral",
    "ISSUES.md is not a changelog",
    "the ISSUES register empties as it works",
    "ISSUES.md entries are deleted when fixed",
    "read ISSUES.md and TECH.md together",
    "the register lives in sdd/ISSUES.md and shrinks as it works",
];

#[test]
fn issues_pattern_catches_every_citation_form() {
    for line in ISSUES_MUST_MATCH {
        assert!(issues_rx().is_match(line), "MISS (should match): {line}");
    }
}

#[test]
fn issues_pattern_leaves_prose_about_the_register_alone() {
    for line in ISSUES_MUST_NOT_MATCH {
        assert!(!issues_rx().is_match(line), "FALSE POSITIVE: {line}");
    }
}

// ── Check 12: Win32 path legality ─────────────────────────────────────────────

/// `PLAN.console.md`, `contents.md` and `nullable.rs` are the negative controls that
/// matter: each CONTAINS a reserved device name and none of them IS one, so a device-name
/// rule written without the anchors would fail the whole tree.
const WIN_ILLEGAL: &[&str] = &[
    "a<b.md",
    "a>b.md",
    "a:b.md",
    "a\"b.md",
    "a|b.md",
    "a?b.md",
    "a*b.md",
    "trailing.",
    "trailing ",
    "con",
    "CON.txt",
    "src/nul",
    "lpt9.md",
    "com1",
    "PRN.md",
    "src/dir/AUX.rs",
    "src/nul/thing.rs",
    "src/com1/x.rs",
    "deep/a/b/nul/c/x.rs",
    "src/nul.txt/x.rs",
    "src/con.foo/x.rs",
    // `lpt0` is illegal and `com0` (below, in WIN_LEGAL) is not. The split is git's, MEASURED
    // on a real NTFS volume — see `device_name_rx` for the table. Keep them in step with that
    // measurement, not with each other: they LOOK like one shape and are not treated as one.
    "src/lpt0/x.rs",
    "lpt0",
    "LPT0.md",
    "a./b.md",
    "a /b.md",
    // The backslash. Win32 reads it as a separator and git refuses the whole tree over it,
    // MEASURED rather than assumed — it was expected to be a silent-divergence case and is
    // not (Windows seat, git 2.49.0.windows.1).
    "a\\b.txt",
    "src/dir\\file.rs",
    // The control character and the newline. Neither survives a heredoc corpus, which is
    // why the shell port could not carry them: the first is invisible in a source listing
    // and the second IS the corpus separator there. Both are ordinary string literals here.
    "a\u{1}b.md",
    "a\nb.md",
];

const WIN_LEGAL: &[&str] = &[
    "ok.md",
    "src/normal-file.rs",
    "contents.md",
    "nullable.rs",
    "PLAN.console.md",
    "sdd/ANTI-PATTERNS.md",
    "a-b_c.rs",
    "Cargo.toml",
    "src/console/x.rs",
    "src/nullable/x.rs",
    "src/a.b/x.rs",
    "src/conin/x.rs",
    "src/conout/x.rs",
    // The COM0 family, and the reason this list has to carry it explicitly: a tracked `com0`
    // CHECKS OUT on Windows and a tracked `lpt0` does not, so these four are the negative
    // half of the one rule in this predicate that is asymmetric on purpose. Without them the
    // asymmetry reads as a typo and the next reader "fixes" it, which is what happened once
    // already. `com10` is here for the adjacent reason — the digit class must not be greedy.
    "com0",
    "COM0.txt",
    "src/com0/x.rs",
    "com10",
];

#[test]
fn win32_predicate_catches_every_illegal_shape() {
    for path in WIN_ILLEGAL {
        assert!(win_illegal_path(path), "MISS (should match): [{path}]");
    }
}

#[test]
fn win32_predicate_admits_legal_paths_that_look_illegal() {
    for path in WIN_LEGAL {
        assert!(!win_illegal_path(path), "FALSE POSITIVE: [{path}]");
    }
}

// ── Check 6a: what a line cites, and how it resolves ──────────────────────────

/// `expected -> line`. The form pins WHAT is extracted, not merely that something matched:
/// the bug this closes was a pattern that matched the wrong substring, which a boolean
/// corpus cannot see.
///
/// The LOWERCASE Rust field access (`plan.switch_to`) is in the negative list for a reason
/// that outlived its origin: the shell gate matched case-sensitively and never saw it,
/// while `Select-String`/`-match` are case-INSENSITIVE by default and its twin did — check
/// 6 failed on Windows and passed on Linux over identical source. One implementation cannot
/// have that split any more, but the case remains the cheapest possible guard against
/// someone adding a case-insensitive flag here.
const CITES: &[(&str, &str)] = &[
    (
        "PLAN.help.md",
        "// Footer status bar (PLAN.help D3): one accessible label",
    ),
    (
        "PLAN.annotations-viewer.md",
        "/// no row (PLAN.annotations-viewer Q1 / TDD 20.2).",
    ),
    (
        "PLAN.copy-path.md",
        "// enabled in connect_open at the point the path is set (PLAN.copy-path D1/D3).",
    ),
    (
        "PLAN.typed-gtk-seams.md",
        "see PLAN.typed-gtk-seams.md for the seam list",
    ),
    ("sdd/PLAN.help.md", "retired: sdd/PLAN.help.md"),
    ("sdd/TECH.md", "the module map in sdd/TECH.md"),
    (
        "tests/MANUAL-TEST.md",
        "the plan lives in tests/MANUAL-TEST.md",
    ),
];

/// Prose mentions that resolve against nothing, the placeholder form, and the one fixture
/// shape that made the extension look unsafe.
///
/// NOTE a directory-qualified path that EXISTS (`sdd/ISSUES.md`) is not a false positive
/// and is deliberately absent: 6a is supposed to extract it, then resolve it and find it
/// present. This corpus is about what must not be extracted AS A PATH AT ALL.
const CITES_NOTHING: &[&str] = &[
    "see POLICY.md for the rule",
    "a PLAN.<topic>.md is deleted by design once implemented",
    "doc_link_fragment(\"./sub/PLAN.md#caf%C3%A9\")",
    "let done = plan.switch_to.is_some() && plan.spot.as_ref();",
];

#[test]
fn citation_extraction_resolves_to_the_document_named() {
    for (expected, line) in CITES {
        let got = citation_targets(line);
        assert_eq!(
            got,
            vec![expected.to_string()],
            "WRONG EXTRACT from: {line}"
        );
    }
}

#[test]
fn citation_extraction_ignores_prose_and_placeholders() {
    for line in CITES_NOTHING {
        let got = citation_targets(line);
        assert!(got.is_empty(), "FALSE POSITIVE: {got:?} from: {line}");
    }
}

// ── Check 8: the citation FORM ────────────────────────────────────────────────

/// The discriminating cases are the tail of each list. A line carrying BOTH a legal and a
/// bare citation must be reported, for the bare one only; the near-misses must not be,
/// because a check with false positives gets disabled while an incomplete one gets
/// extended; and a MISSPELLED prefix must be reported, which is the property the
/// single-token form buys and the retired two-word spelling did not have.
const BARE_AP_MUST_FLAG: &[&str] = &[
    "see AP-79 for the pump-loop lesson",
    "(AP-30 deferred popdown)",
    "per the gtk4-rs skill AP-78 masking",
    "AP-1 leads the router",
    "GTK4Rs/AP-57 / AP-34b enum split",
    "see GTK4Rs/AP-109 and AP-88 together",
    "gtk4rs/AP-9 is a misspelled prefix",
    "GTK4Rs / AP-9 is not one token",
];

const BARE_AP_MUST_NOT_FLAG: &[&str] = &[
    "see GTK4Rs/AP-109 for the tab-close gesture guard",
    "see GTK4Rs/AP-79 for the pump-loop bound",
    "a bare `AP-N` is illegal anywhere in the tree",
    "GTK4Rs/AP-153 pairs with GTK4Rs/AP-153",
    "SOAP-12 is not a citation",
    "the shorthand #79 inside this register",
    "Scr-AP-9 is not a citation either",
    "ScrAP-231 is the resolvable form",
];

#[test]
fn bare_citations_are_flagged() {
    for line in BARE_AP_MUST_FLAG {
        assert!(
            !bare_ap_citations(line).is_empty(),
            "MISS (should flag): {line}"
        );
    }
}

#[test]
fn legal_citations_and_near_misses_are_not_flagged() {
    for line in BARE_AP_MUST_NOT_FLAG {
        assert!(bare_ap_citations(line).is_empty(), "FALSE POSITIVE: {line}");
    }
}

#[test]
fn a_line_with_both_forms_reports_only_the_bare_one() {
    assert_eq!(
        bare_ap_citations("GTK4Rs/AP-57 / AP-34b enum split"),
        vec!["AP-34".to_string()]
    );
}

// ── Check 5b: the paragraph scanner, across a FILE BOUNDARY ───────────────────

/// MULTI-FILE ON PURPOSE (ScrAP-226). 5b was the only check the shell self-test did not
/// exercise, and that is precisely where a defect lived through a mutation-tested release:
/// two file-boundary bugs — a filename read at flush time, and paragraph state carried
/// across a boundary — that CANNOT be reproduced on a single file. The Python cross-check
/// that "validated the algorithm" ran one file at a time and was structurally incapable of
/// seeing either.
///
/// No trailing newline on the first document: an unterminated final paragraph is the
/// trigger for both bugs and is the shape every real document in the prescriptive set has.
fn prescriptive_fixtures() -> Vec<(String, String)> {
    vec![
        (
            "first.md".to_string(),
            "intro\n\nalways annotate with #[gtk::test] first.".to_string(),
        ),
        (
            "second.md".to_string(),
            "Note: gtktest is the harness.\n\ntail\n".to_string(),
        ),
    ]
}

#[test]
fn a_stale_prescription_is_attributed_to_its_own_file() {
    let hits = prose_prescriptions(&prescriptive_fixtures());
    assert_eq!(hits.len(), 1, "expected exactly one hit");
    assert_eq!(hits[0].file, "first.md", "attributed to the wrong file");
    assert_eq!(hits[0].line, 3, "attributed to the wrong line");
}

#[test]
fn a_later_file_cannot_swallow_an_earlier_files_prescription() {
    // The FALSE NEGATIVE, which is the worse half: without the boundary reset, the second
    // file's mention of the replacement clears the first file's pending hit.
    let hits = prose_prescriptions(&prescriptive_fixtures());
    assert!(!hits.is_empty(), "the earlier file's hit was swallowed");
}

#[test]
fn a_paragraph_naming_both_attributes_is_a_contrast_not_an_instruction() {
    let contrast = vec![(
        "contrast.md".to_string(),
        "Never write #[gtk::test]; write #[gtktest::test].\n".to_string(),
    )];
    assert!(
        prose_prescriptions(&contrast).is_empty(),
        "a contrast must not be flagged"
    );
}

// ── Check 11: the growth ratchet does not depend on the checkout ──────────────

/// A Windows checkout of the register is CRLF and a Linux one is LF (`.gitattributes` sets
/// `* text=auto`), so a raw byte count makes the same commit measure ~4,300 bytes larger on
/// one platform. That is a lenient/strict platform split in the gate that exists to prevent
/// exactly those, and it was 383 bytes from firing when this was written.
#[test]
fn the_growth_ratchet_measures_the_same_on_either_line_ending() {
    let lf = "## 1. A heading\n**Symptom**: something\n**Scribobulate**: somewhere\n";
    let crlf = lf.replace('\n', "\r\n");
    assert_eq!(
        crate::lint::checks::register::normalised_bytes(lf),
        crate::lint::checks::register::normalised_bytes(&crlf),
        "the ratchet reads a CRLF working copy as a bigger register than an LF one"
    );
}

// ── The scan set ──────────────────────────────────────────────────────────────

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the xtask crate has a parent directory")
        .to_path_buf()
}

fn real_contract() -> Contract {
    let text = std::fs::read_to_string(repo().join(crate::lint::contract::CONTRACT))
        .expect("the scan contract is readable");
    Contract::parse(&text).expect("the scan contract parses")
}

#[test]
fn the_real_contract_enumerates_a_usable_set() {
    let contract = real_contract();
    let scan = ScanSet::build(&repo(), &contract).expect("the scan set builds");
    assert!(
        scan.paths.len() > 100,
        "the tree enumerated {} files, which is not a working tree",
        scan.paths.len()
    );
}

/// TWO ASSERTIONS, BOTH REQUIRED. Without the first, the second passes vacuously the day
/// the fixture is deleted — which is the exact defect this project already paid for once.
#[test]
fn no_symlink_reaches_the_scan_set() {
    let repo = repo();
    let links: Vec<String> = git_index(&repo)
        .into_iter()
        .filter(|(mode, _)| mode == "120000")
        .map(|(_, path)| path)
        .collect();
    assert!(
        !links.is_empty(),
        "no mode-120000 path is in the git index — the canary fixture \
         (tests/fixtures/escapes-via-symlink.md) is gone, so the next assertion would now \
         pass vacuously"
    );
    let scan = ScanSet::build(&repo, &real_contract()).expect("the scan set builds");
    for link in links {
        assert!(
            !scan.paths.contains(&link),
            "symlink '{link}' (git mode 120000) is in the scan set"
        );
    }
}

/// A file at exactly `maxdepth` is IN the set, and one a level deeper makes the gate REFUSE
/// TO RUN and name it.
///
/// The second half is deliberately "the gate fails and names the path" rather than "the
/// file is absent from the set": absence is also what a broken enumerator produces, so an
/// absence assertion passes for the wrong reason. It is also the behaviour that makes one
/// shared set safe — a budget that quietly drops files is indistinguishable from a tree
/// that has none.
///
/// The probe is planted in a SYNTHETIC tree built from the real contract's own values, not
/// in the working copy: the depth is derived from `maxdepth` rather than hard-coded, and no
/// other test can see the plant. Hard-coding six here would leave the assertion passing for
/// a contract that had moved.
#[test]
fn the_depth_budget_is_a_tripwire_at_exactly_maxdepth() {
    let contract = real_contract();
    let tree = ScratchTree::new(&contract);
    let root = contract
        .roots
        .first()
        .expect("the contract declares a root");

    let mut at_max = format!("{root}/lint-scan-depth-probe");
    for level in 2..contract.maxdepth {
        at_max.push_str(&format!("/d{level}"));
    }
    // Empty on purpose: the probe must be invisible to every other check. A file with
    // content would have to carry a citation or a link.
    at_max.push_str("/at-max.md");
    tree.plant(&at_max);

    let scan = ScanSet::build(tree.path(), &contract).expect("a tree at maxdepth builds");
    assert!(
        scan.paths.contains(&at_max),
        "a file at exactly maxdepth ({}) is MISSING from the scan set: {at_max}\n    \
         The budget is off by one, or the enumeration stops short of it.",
        contract.maxdepth
    );

    let too_deep = at_max.replace("/at-max.md", "/over/too-deep.md");
    tree.plant(&too_deep);
    let refused = ScanSet::build(tree.path(), &contract)
        .expect_err("a file at maxdepth+1 must make the gate refuse to run");
    assert!(
        refused.contains(&too_deep),
        "the gate refused but did not NAME the offending path, so its verdict cannot be \
         attributed to the depth budget: {refused}"
    );
}

#[test]
fn a_garbled_field_is_a_refusal_to_run_not_an_absent_value() {
    // A misspelled field reads as an ABSENT field, and absent is indistinguishable from
    // legitimately empty — so the gate would run over nothing and report a clean tree.
    let garbled = "root src\nprescriptive sdd/POLICY.md\nmaxdepth 6\nrooot tests\n";
    assert!(Contract::parse(garbled).is_err());
}

#[test]
fn an_empty_class_is_a_refusal_to_run() {
    // MEASURED on the shell port: delete the `prescriptive` lines and check 5b reported
    // PASS — over nothing. The way to switch a check off is to delete the check.
    assert!(Contract::parse("root src\nmaxdepth 6\n").is_err());
    assert!(Contract::parse("prescriptive sdd/POLICY.md\nmaxdepth 6\n").is_err());
    assert!(Contract::parse("root src\nprescriptive sdd/POLICY.md\n").is_err());
}

/// A throwaway tree carrying whatever the real contract requires to be present, so a scan
/// can be built over planted files without touching the working copy.
struct ScratchTree(PathBuf);

impl ScratchTree {
    fn new(contract: &Contract) -> ScratchTree {
        let unique = format!(
            "scribobulate-xtask-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        );
        let root = std::env::temp_dir().join(unique);
        let _ = std::fs::remove_dir_all(&root);
        let tree = ScratchTree(root);
        // The prescriptive class is asserted to be a subset of the scan set, so the
        // synthetic tree has to carry those documents for any build to succeed.
        for doc in &contract.prescriptive {
            tree.plant(doc);
        }
        tree
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn plant(&self, relative: &str) {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("the scratch tree is writable");
        }
        std::fs::write(&path, "").expect("the scratch tree is writable");
    }
}

impl Drop for ScratchTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
