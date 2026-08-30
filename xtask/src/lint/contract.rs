//! The shared scan-set contract: ONE enumeration, produced once, consumed by every check.
//!
//! There used to be three, and they disagreed. Measured by planting a Markdown file one
//! level past the budget: check 6a was UNBOUNDED in the bash port and BOUNDED in the
//! PowerShell one, so the shell side caught a dangler at depth 7 and the Windows side did
//! not — a lenient/strict split in the one gate whose whole purpose was to prove the two
//! platforms enforced the same rule. Check 1 walked its own four-root list and disagreed
//! with check 6 IN THE SAME RUN (ScrAP-207).
//!
//! The two-port parity problem is retired with the ports, but the single-enumeration rule
//! is NOT a parity artefact and stays: a check that walks the tree for itself is a second
//! opinion about what the gate covers, and the drift is silent either way. Every check
//! narrows [`ScanSet::paths`]; none touches the filesystem.
//!
//! `scripts/lint-references.scan` remains the contract because the set is DATA — which
//! roots, which loose files, which prefixes are excluded, which documents instruct — and
//! data in a source file is a recompile away from being a fourth opinion.

use crate::lint::patterns::win_illegal_path;
use std::collections::BTreeSet;
use std::path::Path;

pub const CONTRACT: &str = "scripts/lint-references.scan";

/// The contract's fields, parsed but not yet resolved against the filesystem.
pub struct Contract {
    pub roots: Vec<String>,
    pub files: Vec<String>,
    pub prescriptive: Vec<String>,
    pub excludes: Vec<String>,
    pub maxdepth: usize,
}

/// The enumerated set every check draws from.
#[derive(Debug)]
pub struct ScanSet {
    pub paths: Vec<String>,
    pub prescriptive: Vec<String>,
}

impl Contract {
    /// Parse the contract text. Every failure here is a REFUSAL TO RUN rather than an empty
    /// result, because an empty scan set passes every check vacuously and reads identically
    /// to a clean tree.
    pub fn parse(text: &str) -> Result<Contract, String> {
        let mut contract = Contract {
            roots: Vec::new(),
            files: Vec::new(),
            prescriptive: Vec::new(),
            excludes: Vec::new(),
            maxdepth: 0,
        };
        let mut maxdepth_seen = None;
        for (index, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // Reject any line that is not a comment, blank, or a recognised
            // `<field> <value>`. A typo'd field name is otherwise INVISIBLE: it simply
            // fails to match, the field reads as absent, and "absent" is indistinguishable
            // from "legitimately empty" — which is the one failure mode a contract read by
            // the gate cannot afford.
            let mut parts = line.split_whitespace();
            let (Some(field), Some(value), None) = (parts.next(), parts.next(), parts.next())
            else {
                return Err(format!(
                    "{CONTRACT}:{} is neither a comment nor a recognised \
                     'root|file|prescriptive|exclude|maxdepth <value>' pair:\n    {raw}\n  \
                     A misspelled field reads as an ABSENT field, and the gate would then \
                     run over nothing. Fix the line rather than deleting it.",
                    index + 1
                ));
            };
            match field {
                "root" => contract.roots.push(value.to_string()),
                "file" => contract.files.push(value.to_string()),
                "prescriptive" => contract.prescriptive.push(value.to_string()),
                "exclude" => contract.excludes.push(value.to_string()),
                "maxdepth" => {
                    maxdepth_seen = Some(value.parse::<usize>().map_err(|_| {
                        format!("{CONTRACT}: 'maxdepth {value}' is not a whole number")
                    })?);
                }
                other => {
                    return Err(format!(
                        "{CONTRACT}:{} declares an unknown field '{other}'. A misspelled \
                         field reads as an ABSENT field, and the gate would then run over \
                         nothing.",
                        index + 1
                    ))
                }
            }
        }
        contract.maxdepth = match maxdepth_seen {
            Some(depth) if depth > 0 => depth,
            _ => {
                return Err(format!(
                    "{CONTRACT} defines no usable 'maxdepth'. Refusing to run: an empty \
                     scan set would pass every check vacuously."
                ))
            }
        };
        if contract.roots.is_empty() {
            return Err(format!(
                "{CONTRACT} defines no 'root'. Refusing to run: an empty scan set would \
                 pass every check vacuously."
            ));
        }
        // Its own message, because it fails differently. MEASURED on the bash port: delete
        // the `prescriptive` lines and check 5b reported PASS — over nothing. An empty
        // class is a garbled contract, not an empty opinion; the way to switch check 5b off
        // is to delete check 5b.
        if contract.prescriptive.is_empty() {
            return Err(format!(
                "{CONTRACT} defines no 'prescriptive' document. Refusing to run: check 5b \
                 would PASS over an empty set, which reads identically to a clean tree. \
                 Restore the class or remove the check."
            ));
        }
        Ok(contract)
    }
}

impl ScanSet {
    /// Walk the contract's roots and loose files, apply the exclusions, and check the two
    /// tripwires (depth budget, prescriptive membership).
    ///
    /// `repo` is the tree to enumerate. It is a parameter rather than "the current
    /// directory" so the corpus can build a synthetic tree from the REAL contract's values
    /// and assert the depth budget's edges without planting probe files in the working
    /// copy.
    pub fn build(repo: &Path, contract: &Contract) -> Result<ScanSet, String> {
        let symlinks = symlink_paths(repo, contract);
        let mut paths: BTreeSet<String> = BTreeSet::new();

        for root in &contract.roots {
            let dir = repo.join(root);
            if !dir.is_dir() {
                continue;
            }
            // UNBOUNDED on purpose. `maxdepth` is a tripwire, not a filter (see below), so
            // bounding the walk here would hide a file past the budget from the tripwire
            // that exists to name it.
            for entry in walkdir::WalkDir::new(&dir).follow_links(false) {
                let entry = entry.map_err(|why| format!("walking {root}: {why}"))?;
                if entry.file_type().is_file() {
                    if is_desktop_metadata(entry.path()) {
                        continue;
                    }
                    if let Some(rel) = relative(repo, entry.path()) {
                        paths.insert(rel);
                    }
                }
            }
        }
        for file in &contract.files {
            if repo.join(file).exists() {
                paths.insert(file.clone());
            }
        }

        // Excludes are ORDINAL LITERAL prefixes. The bash port escaped them into a regex
        // and the PowerShell one compared them literally, so a metacharacter in a value
        // would have made the two ports scan different trees; there is one implementation
        // now, and a literal prefix is what the contract's values mean.
        paths.retain(|path| {
            !contract
                .excludes
                .iter()
                .any(|prefix| path.starts_with(prefix))
                && !symlinks.contains(path)
        });

        let paths: Vec<String> = paths.into_iter().collect();
        if paths.is_empty() {
            return Err(format!(
                "{CONTRACT} enumerates no file at all. Refusing to run: every check would \
                 PASS over an empty set, which reads identically to a clean tree."
            ));
        }

        // `maxdepth` IS A TRIPWIRE, NOT A FILTER, and that distinction is the point. As a
        // filter it was a silent coverage cliff: a file past the budget was simply absent
        // from the set, and "absent because the budget dropped it" is indistinguishable
        // from "absent because the tree does not contain it". Failing loudly is what makes
        // one shared set safe, and it turns the budget into a statement that the tree has
        // outgrown the contract.
        //
        // Depth counts segments BELOW the root, which is how `find <root> -maxdepth N`
        // counted it. Deriving it from the repo-relative path rather than from the walker
        // retires the off-by-one conversion the two ports were licensed to differ on.
        // `file` entries match no root prefix and are therefore never counted — they are
        // named individually, not reached by a walk.
        let too_deep: Vec<&String> = paths
            .iter()
            .filter(|path| depth_below_roots(path, &contract.roots) > Some(contract.maxdepth))
            .collect();
        if !too_deep.is_empty() {
            let listed = too_deep
                .iter()
                .map(|path| format!("    {path}"))
                .collect::<Vec<_>>()
                .join("\n");
            return Err(format!(
                "these files are deeper than {CONTRACT}'s 'maxdepth {}':\n{listed}\n  \
                 Refusing to run. Raise 'maxdepth' in the contract, or move the files. \
                 Truncating the set instead would make this gate lenient WITHOUT SAYING SO.",
                contract.maxdepth
            ));
        }

        // The `prescriptive` class is a LABEL ON SET MEMBERS, not a fourth enumeration.
        // Asserting membership is what makes it a subset rather than a separate opinion —
        // and it fires the moment a document is marked prescriptive but sits outside every
        // `root`, is excluded, or has been deleted.
        for doc in &contract.prescriptive {
            if !paths.iter().any(|path| path == doc) {
                return Err(format!(
                    "{CONTRACT} marks '{doc}' prescriptive, but that path is not in the \
                     scan set. A prescriptive document is a LABEL on a set member, not a \
                     separate list: check 5b would read a file the enumeration does not \
                     cover. Add a 'root'/'file' entry that contains it, or drop the label."
                ));
            }
        }

        Ok(ScanSet {
            paths,
            prescriptive: contract.prescriptive.clone(),
        })
    }
}

/// `Some(n)` when the path lies under one of the roots, counting segments below it;
/// `None` for a loose `file` entry, which no budget applies to.
fn depth_below_roots(path: &str, roots: &[String]) -> Option<usize> {
    for root in roots {
        if let Some(rest) = path.strip_prefix(&format!("{root}/")) {
            return Some(rest.split('/').count());
        }
    }
    None
}

/// Paths excluded because they are symlinks.
///
/// The union of two tests, each blind on a different platform: the filesystem's own
/// (`is_symlink`), and the git index's mode 120000 — which is what a Windows checkout that
/// materialised the link as a plain file still records. See `scripts/lint-references.scan`,
/// "SYMLINKS ARE EXCLUDED", for why they are out of the set at all.
///
/// Absent git is degraded from, not died on: the filesystem half still applies.
fn symlink_paths(repo: &Path, contract: &Contract) -> BTreeSet<String> {
    let mut excluded = BTreeSet::new();
    for root in &contract.roots {
        let dir = repo.join(root);
        for entry in walkdir::WalkDir::new(&dir)
            .follow_links(false)
            .into_iter()
            .flatten()
        {
            if entry.path_is_symlink() {
                if let Some(rel) = relative(repo, entry.path()) {
                    excluded.insert(rel);
                }
            }
        }
    }
    for file in &contract.files {
        if repo.join(file).is_symlink() {
            excluded.insert(file.clone());
        }
    }
    for (mode, path) in git_index(repo) {
        if mode == "120000" {
            excluded.insert(path);
        }
    }
    excluded
}

/// `git ls-files -s` as (mode, path) pairs, empty outside a checkout.
pub fn git_index(repo: &Path) -> Vec<(String, String)> {
    let Ok(out) = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["ls-files", "-s"])
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let (meta, path) = line.split_once('\t')?;
            let mode = meta.split_whitespace().next()?;
            Some((mode.to_string(), path.to_string()))
        })
        .collect()
}

/// Every tracked path, whole, with the enumerator's exit status — check 12's input.
///
/// `-z` and a NUL-boundary read, both halves required. A path containing a NEWLINE is torn
/// into two fragments by any line-oriented read, each matching nothing, so the gate
/// silently misses the class it is most likely to meet (MEASURED: an unquoted
/// `sed -i 's|a|b|'` wrote the replacement half to disk as a filename and `git add -A`
/// committed it). And the status is READ rather than assumed: a failed enumeration that
/// reads as an empty one is a check that proved nothing while printing PASS.
pub fn tracked_paths(repo: &Path) -> Result<Vec<String>, String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["ls-files", "-z"])
        .output()
        .map_err(|why| format!("'git ls-files' could not be run ({why}); the tree was not enumerated, so check 12 would prove nothing"))?;
    if !out.status.success() {
        return Err(format!(
            "'git ls-files' exited {}; the tree was not enumerated, so check 12 would \
             prove nothing. A silent empty input reads exactly like a clean tree.",
            out.status
        ));
    }
    Ok(out
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8_lossy(path).into_owned())
        .collect())
}

/// A repo-relative path with `/` separators, which is what the contract, `git ls-files` and
/// every citation in the tree all speak — including on Windows, where the walker does not.
fn relative(repo: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(repo).ok()?;
    Some(rel.to_string_lossy().replace('\\', "/"))
}

/// Check 12's report, kept here so the enumerator and its predicate stay adjacent.
pub fn illegal_tracked_paths(repo: &Path) -> Result<Vec<String>, String> {
    Ok(tracked_paths(repo)?
        .into_iter()
        .filter(|path| win_illegal_path(path))
        .collect())
}

/// Desktop-manager metadata a file browser drops into any directory it is pointed at.
///
/// SKIPPED BY BASENAME, NOT BY AN `exclude` PREFIX, and the distinction is the whole point.
/// These files are gitignored and machine-dependent — whether one exists depends on which
/// directories a developer happened to open in Finder — which is exactly the class the
/// contract's `exclude` keyword describes: a member whose presence makes the gate's verdict
/// a fact about the machine rather than about the commit. But an `exclude packaging/.DS_Store`
/// line fixes only the directory someone has already browsed; the next one appears under
/// `src/` or `data/` the moment anyone browses there. The hazard is a property of the FILE,
/// so the skip is too — the same reasoning that put the symlink rule in code rather than in
/// a contract line.
///
/// FOUND BY THE THREE-PLATFORM SCAN-SET COMPARISON, on its first real run: macOS enumerated
/// 433 members where Linux enumerated 432, and the difference was `packaging/.DS_Store`. That
/// is the divergence POLICY § "Continuous integration" keeps the parity job for, and it is
/// worth recording that retiring the two shell ports did NOT remove it — one implementation
/// removes port divergence, and this was the filesystem answering differently.
///
/// The latent half, INFERRED and not measured: `.DS_Store` is binary and stores filenames,
/// and set members are read as text. A path- or citation-shaped token inside one could raise
/// a finding that only a macOS seat can reproduce. Skipping the file forecloses that too.
fn is_desktop_metadata(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".DS_Store")
    )
}
