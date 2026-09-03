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
use crate::lint::patterns::{
    bare_ap_citations, declares_whole_id, issues_rx, prose_prescriptions, reverse_dns_rx,
    win_illegal_path,
};
use crate::lint::patterns::{marker_reason, parser_dispatch_wildcards};
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

// ── Check 14: the .ps1 encoding predicate discriminates ───────────────────────

/// TWO FIXTURES, BOTH REQUIRED, and their EXISTENCE asserted before their content. The
/// fixtures are deliberately malformed and therefore excluded from the scan set
/// (`scripts/lint-references.scan`), so nothing else in this tree reads them: delete one
/// and the assertion below it would otherwise pass on an empty file, which is how a gate
/// stops gating without going red.
#[test]
fn the_ps1_encoding_predicate_discriminates() {
    let bad = repo().join("tests/fixtures/encoding/bomless-nonascii.ps1");
    let good = repo().join("tests/fixtures/encoding/bom-nonascii.ps1");
    let bad_bytes = std::fs::read(&bad).unwrap_or_else(|_| panic!("{bad:?} is readable"));
    let good_bytes = std::fs::read(&good).unwrap_or_else(|_| panic!("{good:?} is readable"));

    assert!(
        crate::lint::checks::architecture::ps1_encoding_violation(&bad_bytes).is_some(),
        "a BOM-less .ps1 carrying a byte above 0x7F must be refused"
    );
    assert!(
        crate::lint::checks::architecture::ps1_encoding_violation(&good_bytes).is_none(),
        "a BOM makes non-ASCII legal; the invariant is BOM OR pure ASCII, not ASCII alone"
    );
    // The good fixture must actually CARRY non-ASCII, or it proves only that ASCII passes.
    assert!(
        good_bytes[3..].iter().any(|byte| *byte > 0x7F),
        "the BOM fixture has no non-ASCII byte after its BOM, so it tests nothing"
    );
}

// ── Desktop metadata never reaches the scan set ───────────────────────────────

/// The macOS seat's `.DS_Store` finding, pinned so the skip cannot be dropped.
///
/// TWO ASSERTIONS, BOTH REQUIRED, for the reason the symlink case states: without the
/// control, the skip assertion passes vacuously the day the predicate widens into skipping
/// everything, or the day the walk stops finding anything at all.
///
/// PLANTED IN A SCRATCH TREE, never in the working copy — this repository is developed on
/// Linux, so the file this defends against is one no Linux seat will have lying around to
/// notice a regression with. That asymmetry is the entire reason the skip needs a test
/// rather than a comment.
#[test]
fn desktop_metadata_is_skipped_and_ordinary_files_are_not() {
    let contract = real_contract();
    let tree = ScratchTree::new(&contract);
    let root = contract
        .roots
        .first()
        .expect("the contract declares a root");

    tree.plant(&format!("{root}/.DS_Store"));
    tree.plant(&format!("{root}/lint-scan-control.md"));

    let scan = ScanSet::build(tree.path(), &contract).expect("the scratch tree builds");

    assert!(
        scan.paths
            .iter()
            .any(|path| path.ends_with("lint-scan-control.md")),
        "the control file did not reach the set, so this proves nothing about the skip"
    );
    assert!(
        !scan.paths.iter().any(|path| path.contains(".DS_Store")),
        "desktop metadata reached the scan set"
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

// ── Check 7: the app ID, whole rather than merely present ─────────────────────

/// The canonical ID, as `src/icons.rs` declares it. A literal here on purpose: the corpus
/// must keep testing the shape even if the real ID ever changes.
const CANON: &str = "com.extollit.scribobulate";

/// The blind spot this pair of predicates was written to close.
///
/// `com.extollit.scribobulated` passed check 7 whole: `contains` is true for any superset
/// of the canonical ID, and the old `…scribobulate\b` pattern could not match it because
/// the trailing `d` is a word character and killed the boundary. Both halves are asserted
/// here because closing either one alone leaves the gate reporting the wrong thing.
#[test]
fn an_id_that_merely_extends_the_canonical_is_not_the_canonical() {
    let drifted = "com.extollit.scribobulated";
    assert!(
        !declares_whole_id(drifted, CANON),
        "presence half: a superset must not satisfy the ID"
    );
    let foreign: Vec<&str> = reverse_dns_rx()
        .find_iter(drifted)
        .map(|m| m.as_str())
        .filter(|f| *f != CANON)
        .collect();
    assert_eq!(
        foreign,
        [drifted],
        "foreign half: the extended id must be reported"
    );
}

#[test]
fn a_real_declaration_still_satisfies_the_presence_half() {
    for text in [
        CANON,
        "  <file>com.extollit.scribobulate.svg</file>",
        "Icon=com.extollit.scribobulate\nStartupWMClass=com.extollit.scribobulate",
        "install -Dm644 \"$D/com.extollit.scribobulate.svg\" \"$I/\"",
        "<string>com.extollit.scribobulate</string>",
        "/com/extollit/scribobulate/ and com.extollit.scribobulate together",
    ] {
        assert!(declares_whole_id(text, CANON), "FALSE NEGATIVE: {text}");
    }
}

/// The other two drift directions, which the widened pattern must not have lost.
#[test]
fn prefix_and_last_segment_drift_are_still_caught() {
    // A different vendor prefix: reported as foreign.
    let other = "org.other.scribobulate";
    let foreign: Vec<&str> = reverse_dns_rx()
        .find_iter(other)
        .map(|m| m.as_str())
        .filter(|f| *f != CANON)
        .collect();
    assert_eq!(foreign, [other]);
    // A truncated last segment: the ID is simply absent.
    assert!(!declares_whole_id("com.extollit.scribobulat", CANON));
}

/// A `.` must NOT continue the identifier, and this is the case that decided it: the ID is
/// also a filename, so the icon's `.svg` is an extension. Asserted so the trade is not
/// silently reversed by someone tightening the predicate.
#[test]
fn an_extension_does_not_make_the_id_a_different_one() {
    assert!(declares_whole_id("com.extollit.scribobulate.svg", CANON));
    assert!(declares_whole_id("com.extollit.scribobulate.png", CANON));
}

// ── check: parser-vocabulary dispatchers are exhaustive ──────────────────────
//
// The gate on this gate. The predicate has to fire on the shape that actually
// shipped (`copymap::classify`'s `_ => return None`) and stay silent on the three
// shapes that look like it and are not: an exhaustive dispatcher, a wildcard in a
// match over some OTHER type, and the vocabulary named in prose.

#[test]
fn wildcard_in_a_parser_dispatch_is_caught() {
    // The exact shape that let raw HTML acquire no copy node.
    let src = r#"
fn classify(ev: &Event) -> Option<RawKind> {
    Some(match ev {
        Event::Text(t) => RawKind::Text(t.to_string()),
        Event::Rule => RawKind::Atomic,
        _ => return None,
    })
}
"#;
    assert_eq!(
        parser_dispatch_wildcards(src),
        vec![6],
        "the `_ =>` arm must be found"
    );
}

/// **F-AP-B-102: a ≥30-line CRLF file and its LF twin must report the same line.**
///
/// `text.lines()` strips both bytes of a `\r\n` while the walk's own offset
/// accumulator adds one for the terminator, so on a CRLF checkout the accumulator falls
/// one character behind PER LINE. Past about thirty lines the drift exceeds the
/// indentation the depth model is looking at, the check sees no `match` block at all,
/// and it reports PASS having found nothing.
///
/// **Silent, green, and green only where nobody looks**: Windows is the one platform
/// whose `.gitattributes` guarantees CRLF. The fixture has to be long, because a short
/// one drifts by too little to change the answer — which is why nothing caught this.
///
/// The fold now happens at the reader (`lint::read_text`), so this case exercises the
/// walk's own arithmetic directly rather than the reader's.
#[test]
fn a_crlf_file_reports_the_same_line_as_its_lf_twin() {
    let mut lf = String::from("\nfn classify(ev: &Event) -> Option<RawKind> {\n");
    // Padding ABOVE the match, so the accumulator has somewhere to drift.
    for i in 0..40 {
        lf.push_str(&format!("    // filler line {i}, long enough to matter\n"));
    }
    lf.push_str("    Some(match ev {\n");
    lf.push_str("        Event::Text(t) => RawKind::Text(t.to_string()),\n");
    lf.push_str("        Event::Rule => RawKind::Atomic,\n");
    lf.push_str("        _ => return None,\n");
    lf.push_str("    })\n}\n");

    let found = parser_dispatch_wildcards(&lf);
    assert_eq!(found.len(), 1, "the LF twin finds the wildcard: {found:?}");

    let crlf = lf.replace('\n', "\r\n");
    assert_eq!(
        parser_dispatch_wildcards(&crlf),
        found,
        "and the CRLF twin finds it on the SAME line. A gate that reports PASS having \
         found nothing is indistinguishable from one that passed, and this one did so \
         on the only platform that guarantees these line endings"
    );
}

#[test]
fn a_guarded_wildcard_is_caught_too() {
    // `_ if cond =>` is a wildcard wearing a hat; it still absorbs every unnamed
    // variant that satisfies the guard.
    let src = r#"
match ev {
    Tag::Paragraph => a(),
    _ if flag => b(),
}
"#;
    assert_eq!(parser_dispatch_wildcards(src), vec![4]);
}

#[test]
fn a_brace_inside_a_char_literal_does_not_move_the_depth() {
    // MEASURED blind spot: the depth model had no arm to ENTER a char literal, so `'{'`
    // counted as an opening brace and every dispatch below it sat at the wrong depth —
    // the check then saw no `match` block at all and passed vacuously. A file holding
    // `'{'` or `'}'` is not exotic: any hand-rolled lexer has one, and this tree's own
    // raw-HTML scanners are full of them.
    // The literal must sit INSIDE the match block, and be unbalanced there. The model is
    // relative — arms are recognised at the `match` line's depth plus one — so a stray
    // brace ABOVE the block shifts every level equally and nothing breaks. Put one in an
    // early ARM and the later arms sit a level too deep to be recognised at all, which
    // is how the check went silently blind rather than noisily wrong. Both facts were
    // learned by mutation: the first draft of this case used a balanced `'{' || '}'`
    // above the block and survived removal of the fix. GEP-1 holds the general rule —
    // plant the violation where the mechanism CONSUMES it, and establish first what
    // quantity it measures and from which anchor.
    let src = r#"
match ev {
    Event::Text(t) => lex(t, '{'),
    _ => other(),
}
"#;
    assert_eq!(
        parser_dispatch_wildcards(src),
        vec![4],
        "an arm carrying a brace-bearing char literal does not hide the wildcard below it"
    );
}

#[test]
fn a_lifetime_is_not_read_as_a_char_literal() {
    // The other half of the same arm, and the reason it cannot be a bare `'` test: a
    // lifetime shares the delimiter, so treating every `'` as an opener blanks the rest
    // of the file from the first `&'a` — trading one blind spot for a larger one.
    // The arm carries a REAL brace pair that must still be counted. Under a naive
    // `c == '\''` opener the lifetime opens a literal with no closing quote after it, so
    // everything to end-of-file goes inert — the arm's own `}` included — and the
    // wildcard below is never seen.
    let src = r#"
match ev {
    Event::Text(t) => { helper::<'a>(t) }
    _ => rest(),
}
"#;
    assert_eq!(
        parser_dispatch_wildcards(src),
        vec![4],
        "a lifetime inside an arm leaves the wildcard below it visible"
    );
}

#[test]
fn an_exhaustive_parser_dispatch_is_clean() {
    let src = r#"
match ev {
    Event::Text(t) => one(t),
    Event::Rule | Event::TaskListMarker(_) => two(),
    Event::Html(_) => three(),
}
"#;
    assert!(parser_dispatch_wildcards(src).is_empty());
}

#[test]
fn a_wildcard_over_some_other_type_is_not_this_checks_business() {
    // Most matches in the tree legitimately end in `_`. A check that flagged them all
    // would be turned off within a day.
    let src = r#"
match colour {
    Colour::Red => a(),
    _ => b(),
}
"#;
    assert!(parser_dispatch_wildcards(src).is_empty());
}

#[test]
fn a_nested_wildcard_does_not_indict_its_parent() {
    // The inner match is over another vocabulary and keeps its own wildcard; the
    // outer one is exhaustive and must stay clean.
    let src = r#"
match ev {
    Event::Text(t) => match kind {
        Kind::A => a(),
        _ => b(),
    },
    Event::Rule => c(),
}
"#;
    assert!(
        parser_dispatch_wildcards(src).is_empty(),
        "the inner match's wildcard belongs to the inner match"
    );
}

#[test]
fn the_vocabulary_named_in_prose_does_not_make_a_dispatcher() {
    // Comments and strings routinely name these types — this module does it above.
    let src = r#"
match colour {
    // Event::Text is not handled here, see the renderer.
    Colour::Red => a(),
    _ => b(),
}
"#;
    assert!(parser_dispatch_wildcards(src).is_empty());
}
#[test]
fn two_sequential_dispatchers_are_both_reported() {
    let src = r#"
fn a(tag: &Tag) -> Option<C> {
    Some(match tag {
        Tag::Emphasis => C::E,
        _ => return None,
    })
}

fn b(end: TagEnd) -> Option<C> {
    Some(match end {
        TagEnd::Emphasis => C::E,
        _ => return None,
    })
}
"#;
    assert_eq!(parser_dispatch_wildcards(src), vec![5, 12]);
}

#[test]
fn a_dispatcher_following_a_block_arm_is_still_seen() {
    // REGRESSION. The first version of this predicate reported `construct_of_tag`'s
    // wildcard and silently missed `construct_of_tagend`'s, 18 lines below and
    // textually identical — found by mutation-testing the gate against the real tree,
    // not by its corpus, which is the failure this file exists to prevent.
    let src = r#"
fn classify(ev: &Event) -> Option<RawKind> {
    Some(match ev {
        Event::Text(t) => RawKind::Text(t.to_string()),
        Event::InlineMath(_) | Event::DisplayMath(_) => {
            return None
        }
    })
}

fn construct_of_tag(tag: &Tag) -> Option<C> {
    Some(match tag {
        Tag::Emphasis => C::E,
        _ => return None,
    })
}

fn construct_of_tagend(end: TagEnd) -> Option<C> {
    Some(match end {
        TagEnd::Emphasis => C::E,
        _ => return None,
    })
}
"#;
    assert_eq!(
        parser_dispatch_wildcards(src),
        vec![14, 21],
        "both wildcards must be reported, not just the first"
    );
}

#[test]
fn a_type_whose_name_ends_in_event_is_not_a_parser_dispatch() {
    // REGRESSION, found by running the gate over the real tree: GIO's
    // `FileMonitorEvent::AttributeChanged` contains the substring "Event::". That type
    // is `#[non_exhaustive]`, so its wildcard is REQUIRED — flagging it would demand
    // code that does not compile, and a check that does that gets disabled.
    let src = r#"
match event {
    FileMonitorEvent::Changed => a(),
    _ => b(),
}
"#;
    assert!(parser_dispatch_wildcards(src).is_empty());
}

#[test]
fn a_multibyte_comment_does_not_desynchronise_the_scan() {
    // REGRESSION. `depth` is indexed by CHARACTER; accumulating byte lengths drifts
    // the moment a non-ASCII character appears, and this tree's comments are full of
    // box-drawing rules. The first version reported the first dispatcher in a file and
    // silently missed the next — invisible to an ASCII-only corpus.
    let src = "
// ── a box-drawing rule — with an em dash ──
match ev {
    Event::Text(t) => one(t),
    _ => return None,
}
";
    assert_eq!(parser_dispatch_wildcards(src), vec![5]);
}

// ── the `dispatch-selector:` opt-out (check 15's accept-with-reason path) ──
//
// The design ratified over blanket-failing every wildcard: check 15 no longer tries to
// tell a selector from a dispatcher by the ARM'S SHAPE (`_ if cond =>` reads like a
// selector and IS one here, but a bare `_ =>` can be either, and a match's true nature
// is not visible to a text scan). Instead a wildcard is accepted only when it carries
// the marker with a real reason; unmarked stays refused by default. `marker_reason`
// is the predicate the annotation scan calls, so a corpus case here is evidence about
// what the CHECK does, not a re-typed copy of it.

#[test]
fn marker_reason_requires_nonempty_text_after_the_token() {
    assert_eq!(
        marker_reason("    // dispatch-selector: gates on caller state, not variant"),
        Some("gates on caller state, not variant")
    );
    assert_eq!(
        marker_reason("    _ => {} // dispatch-selector: same-line form"),
        Some("same-line form")
    );
    // A bare marker — copied without writing the sentence it stands for — does not
    // count, same limit as check 8's citation FORM rule: presence is checkable,
    // truth is not, so at minimum something must have been written.
    assert_eq!(marker_reason("    // dispatch-selector:"), None);
    assert_eq!(marker_reason("    // dispatch-selector:    "), None);
    assert_eq!(marker_reason("    _ => {}"), None);
}

#[test]
fn a_marker_on_the_wildcards_own_line_is_accepted() {
    let src = r#"
match ev {
    Event::Text(t) => one(t),
    _ if active => two(), // dispatch-selector: selects on `active`, not on which
                           // variant arrived; classify() above already named it
    _ => {} // dispatch-selector: everything outside the active span is irrelevant here
}
"#;
    assert!(
        parser_dispatch_wildcards(src).is_empty(),
        "both wildcards carry a same-line reason and must be accepted"
    );
}

#[test]
fn a_marker_directly_above_the_wildcard_is_accepted() {
    let src = r#"
match ev {
    Event::Text(t) => one(t),
    // dispatch-selector: this arm selects by table-cell activity computed above the
    // match, never by which Event/Tag/TagEnd variant is under it.
    _ if active => two(),
}
"#;
    assert!(
        parser_dispatch_wildcards(src).is_empty(),
        "a marker on the comment run directly above the arm must be accepted"
    );
}

#[test]
fn an_unmarked_wildcard_still_fails_by_default() {
    // The control for the two cases above: same shape, no marker anywhere — refused,
    // exactly as before this feature existed. Accept-with-reason must not become
    // accept-unconditionally by accident.
    let src = r#"
match ev {
    Event::Text(t) => one(t),
    _ if active => two(),
}
"#;
    assert_eq!(parser_dispatch_wildcards(src), vec![4]);
}

#[test]
fn a_bare_marker_with_no_reason_does_not_excuse_the_wildcard() {
    let src = r#"
match ev {
    Event::Text(t) => one(t),
    // dispatch-selector:
    _ if active => two(),
}
"#;
    assert_eq!(
        parser_dispatch_wildcards(src),
        vec![5],
        "a marker with nothing after the colon is indistinguishable from a copied \
         token and must not exempt the arm"
    );
}

#[test]
fn a_marker_separated_by_a_blank_line_does_not_reach_the_wildcard() {
    // Tight binding on purpose: without the no-gap rule this marker could be read as
    // excusing the WRONG arm once the code beneath either one moves.
    let src = r#"
match ev {
    Event::Text(t) => one(t),
    // dispatch-selector: explains the arm below, or so it looks

    _ if active => two(),
}
"#;
    assert_eq!(
        parser_dispatch_wildcards(src),
        vec![6],
        "a blank line between the marker and the arm must not exempt it"
    );
}

#[test]
fn a_marker_on_a_different_arm_does_not_excuse_this_one() {
    // Two wildcards in one dispatcher, only one carrying a reason — the annotation is
    // PER ARM, not per match block.
    let src = r#"
match ev {
    Event::Text(t) => one(t),
    _ if active => two(), // dispatch-selector: selects on caller state
    _ => three(),
}
"#;
    assert_eq!(
        parser_dispatch_wildcards(src),
        vec![5],
        "only the unmarked arm should be reported"
    );
}
