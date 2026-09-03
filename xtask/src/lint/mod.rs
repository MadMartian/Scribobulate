//! Cross-reference gate — build-pipeline step 9. POLICY § "Build pipeline" states the
//! rule; THIS CRATE is the source of truth for what is checked.
//!
//! DELIBERATELY NO COUNT HERE, and no count in POLICY.md either. The bash port's header
//! said "Eight mechanical checks" and POLICY said "nine" while the script implemented
//! FOURTEEN — the same defect the coverage floor and the input limits are each written up
//! under, committed by the two places that describe this gate. A count is the one fact
//! about a growing list guaranteed to be wrong by the next addition. The run output
//! enumerates every check by number and title as it executes; that enumeration IS the list,
//! and it cannot drift from what ran because the checks emit it themselves.
//!
//! The classes, which do not change when a check is added: citations the codebase makes
//! into the SDD registers; the test architecture, whose failure modes are silent (that the
//! main-thread GTK suite's duplicated module list has not drifted, and that the superseded
//! `#[gtk::test]` attribute has neither returned nor been prescribed in prose); document
//! paths, that every file this tree points at still exists; the application ID, which
//! several non-Rust packaging files restate and none derives; the citation FORM itself;
//! the registers' own integrity (number immutability, TOC/body agreement, growth); path
//! legality, that no tracked path is one Windows refuses to check out; and the
//! VOCABULARIES the shipped manuals restate -- the theme keys, the config settings and the
//! About dialog's attribution -- none of which any build can see, and all of which a
//! reader outside this project takes as the truth about the program.
//!
//! WHY THIS EXISTS. Two of those classes were found in one session, in a single 103-line
//! change, by human review — after `cargo fmt`, `cargo clippy -D warnings` and 625 passing
//! tests had all gone green. No compiler or test can see a wrong reference: every other
//! gate passes identically whether the citations are right or wrong.
//!
//! WHAT IT CANNOT DO — read this before trusting a PASS. Check 2 proves a cited entry
//! EXISTS, never that it is the RIGHT one. The error that motivated the original script
//! cited a real entry about a menu-button startup freeze for a claim only the popover
//! unparenting entry supports. Both checks pass on that. The only guard is to place a
//! number against the claim it supports and to re-read it when copying a sibling comment.
//!
//! AND A GATE MUST BE PROVEN TO FIRE. The first version of check 1 shipped with a pattern
//! that missed three real citation forms — including the exact form of one of the two
//! defects that motivated writing it. That is what the corpora in `corpus.rs` are: a gate
//! on the gate, run by `cargo test` (build-pipeline step 4) rather than by a bespoke
//! `--self-test` mode, because a hand-rolled corpus runner is exactly where the defects
//! concentrated when this gate was two shell scripts.

pub mod checks;
mod contract;
// The corpora are test-only: they exist to prove the predicates discriminate, and they are
// the one file this gate excludes from its own text checks.
#[cfg(test)]
mod corpus;
// The man-page corpora. A file of their own: corpus.rs is already twice the size limit,
// and it carries a self-exclusion these cases do not need.
#[cfg(test)]
mod corpus_manpage;
mod patterns;
pub mod vocab;

use contract::{Contract, ScanSet};
use std::path::{Path, PathBuf};

/// The files the checks READ, over and above the scan set.
///
/// Guarding an extraction against "no match" makes an empty result a value rather than an
/// abort — but it cannot distinguish "this file has no entries" from "this file is not
/// there", and a MISSING register would make check 3 compare nothing against nothing and
/// print PASS. A gate that reports success because its input vanished is the worst outcome
/// available to it. Check existence once, explicitly, and name the file.
/// A SLICE, not a sized array: the count is a fact about a growing list, and the one thing
/// guaranteed to be wrong by the next addition — the same reason nothing here counts the
/// checks.
const REQUIRED: &[&str] = &[
    "sdd/ANTI-PATTERNS.md",
    "sdd/ISSUES.md",
    "sdd/SCHEMA.md",
    "src/icons.rs",
    "src/lib.rs",
    "src/gtk_suite.rs",
    // Checks 16 and 17 compare these against each other. A missing one would make a
    // vocabulary comparison compare nothing against nothing, which is the outcome this
    // list exists to make impossible.
    "src/theme/keys.rs",
    "src/config.rs",
    "src/app/appactions.rs",
    "packaging/man/scribobulate.1",
    "packaging/man/scribobulate.5",
];

/// The one path this gate excludes from its own text checks: the corpus module quotes the
/// very citation forms checks 1, 6a and 8 hunt for, exactly as a linter's own fixtures do.
/// ONE path, deliberately — the shell ports had to exclude each other as well as
/// themselves, and the pair had to stay in step by hand.
pub const CORPUS_FILE: &str = "xtask/src/lint/corpus.rs";

/// The tree the checks run over: the enumerated scan set with every member's text already
/// read, so no check walks the filesystem or re-reads a file for itself.
pub struct Tree {
    pub repo: PathBuf,
    pub scan: ScanSet,
    texts: Vec<(String, Option<String>)>,
}

impl Tree {
    /// The text of a scan-set member, or `None` when it is binary (or unreadable).
    ///
    /// Binary members are skipped the way `grep` skips them — a NUL byte near the start —
    /// rather than being scanned as lossy text. `data/` and `tests/` carry PNG, JPEG and
    /// ICO files that are legitimately in the set (check 12 and the depth tripwire want
    /// them), and a citation pattern firing on compressed bytes would be a false positive
    /// nobody could act on.
    pub fn text(&self, path: &str) -> Option<&str> {
        self.texts
            .iter()
            .find(|(name, _)| name == path)
            .and_then(|(_, text)| text.as_deref())
    }

    /// Every readable member, in scan-set order.
    pub fn texts(&self) -> impl Iterator<Item = (&str, &str)> {
        self.texts
            .iter()
            .filter_map(|(name, text)| text.as_deref().map(|text| (name.as_str(), text)))
    }

    /// The readable members that pass `keep`, in scan-set order.
    pub fn subset_texts(&self, keep: impl Fn(&str) -> bool + Copy) -> Vec<(&str, &str)> {
        self.texts().filter(|(name, _)| keep(name)).collect()
    }

    pub fn exists(&self, path: &str) -> bool {
        self.repo.join(path).exists()
    }
}

/// Run the gate. `Ok(true)` is a pass, `Ok(false)` a failure of the tree, `Err` a refusal
/// to run — the three states a gate has, kept distinct because a gate that cannot run is
/// worse than one that is absent when its red reads as a verdict.
pub fn run() -> Result<bool, String> {
    let repo = repo_root()?;
    let contract_text = std::fs::read_to_string(repo.join(contract::CONTRACT)).map_err(|why| {
        format!(
            "{} is missing or unreadable ({why}) — it is the scan-set contract this gate \
             reads. Without it there is no file set to check, which is NOT the same as a \
             clean tree.",
            contract::CONTRACT
        )
    })?;
    let contract = Contract::parse(&contract_text)?;
    let scan = ScanSet::build(&repo, &contract)?;

    for required in REQUIRED {
        if !repo.join(required).exists() {
            return Err(format!(
                "{required} is missing — this gate reads it. Refusing to run: checks over a \
                 file that is not there report PASS."
            ));
        }
    }

    let texts = scan
        .paths
        .iter()
        .map(|path| (path.clone(), read_text(&repo.join(path))))
        .collect();
    let tree = Tree { repo, scan, texts };

    Ok(checks::run_all(&tree))
}

/// The scan set, one path per line, ordinal-sorted — the artefact the three platforms are
/// diffed on (POLICY § "Continuous integration"). Built exactly as `run` builds it, from the
/// same contract, so the listing cannot describe a set the checks do not run over.
///
/// ORDINAL SORT, not the platform's collation: a locale-aware sort makes the same set print
/// in a different order on a different host, and a parity diff cannot tell that apart from a
/// set that genuinely differs. The ONE difference between three ports and one binary is that
/// this ordering is now a property of the program rather than of whichever `sort` was first
/// on PATH (ScrAP-319).
pub fn list_scan() -> Result<String, String> {
    let repo = repo_root()?;
    let contract_text = std::fs::read_to_string(repo.join(contract::CONTRACT))
        .map_err(|why| format!("{} is missing or unreadable ({why})", contract::CONTRACT))?;
    let contract = Contract::parse(&contract_text)?;
    let mut paths = ScanSet::build(&repo, &contract)?.paths;
    paths.sort_unstable();
    Ok(paths
        .into_iter()
        .map(|path| format!("{path}\n"))
        .collect::<String>())
}

/// The repository root: this crate's manifest directory's parent, so the gate is
/// independent of the working directory a runner happens to invoke it from. `cargo xtask`
/// sets `CARGO_MANIFEST_DIR` for the xtask crate, never for the caller.
fn repo_root() -> Result<PathBuf, String> {
    let manifest = std::env::var("CARGO_MANIFEST_DIR")
        .map_err(|_| "CARGO_MANIFEST_DIR is unset — run this through `cargo xtask`".to_string())?;
    Path::new(&manifest)
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("{manifest} has no parent directory"))
}

/// A file's text, or `None` when it is binary or unreadable.
///
/// **Line endings are folded to `\n` here, at the READER, so no check has to remember**
/// (F-AP-B-102). A check that walks `text.lines()` while accumulating its own character
/// offset gets one character less per line than the file holds on a CRLF checkout —
/// `lines()` strips both bytes and the accumulator adds one — and the offset then
/// desynchronises from every position-indexed array the check built. Check 15 did
/// exactly that: after about thirty lines its depth lookup was meaningless, so it found
/// nothing and reported PASS.
///
/// **On the one platform whose `.gitattributes` guarantees CRLF**, which is what makes
/// this the worst shape a gate can take: it is silent, it is green, and it is green
/// only where nobody looks. Every check here reports LINE NUMBERS, and folding
/// preserves the line count and every line's index, so nothing a check says about a
/// file changes — only the arithmetic between the lines does.
pub(crate) fn read_text(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.iter().take(8192).any(|byte| *byte == 0) {
        return None;
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Some(if text.contains('\r') {
        text.replace("\r\n", "\n")
    } else {
        text
    })
}
