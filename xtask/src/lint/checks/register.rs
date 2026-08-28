//! The register-integrity checks: 9, 10, 11 and 13. All four are about
//! `sdd/ANTI-PATTERNS.md` keeping the promises the rest of the tree cites it on.

use super::{fail, header, pass};
use crate::lint::patterns as rx;
use crate::lint::Tree;
use std::collections::{BTreeMap, BTreeSet};

const REGISTER: &str = "sdd/ANTI-PATTERNS.md";
const MANIFEST: &str = "sdd/scrap-numbers.manifest";

/// Check 9 — ScrAP numbers are frozen IDs: never renumbered, never reused, and a deleted or
/// merged entry keeps a landing-spot stub under its heading forever.
///
/// Until this existed the rule was enforced by a person hand-diffing the heading set against
/// the shared branch through a migration that rewrote 80% of the file — which worked, and is
/// exactly the kind of guarantee that stops working the first time nobody remembers to run
/// it. It matters more since the citation sweep: hundreds of comments in `src/` cite a
/// register by number, and a silently dropped heading breaks working citations in both
/// directions.
///
/// THE MANIFEST IS A DIFF SEED, NOT A SNAPSHOT. It was generated from the shared branch,
/// where the heading set is independently known good, NOT from the working file.
/// Regenerating it from whatever the file currently says would bless a heading that had
/// already gone missing and hold the gate green forever after — a check that cannot fail,
/// built that way at construction time. So: to ADD an entry, append its number. NEVER
/// regenerate this file wholesale.
pub fn number_immutability(tree: &Tree) -> bool {
    header(
        "9",
        "ScrAP number immutability (no removed, renamed or reused numbers)",
    );
    let Some(manifest) = tree.text(MANIFEST) else {
        return fail(
            &format!("manifest {MANIFEST} is missing; number immutability is unenforced"),
            &[],
            &[],
        );
    };
    let allocated: BTreeSet<&str> = manifest
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();

    let mut present: Vec<String> = Vec::new();
    for line in tree.text(REGISTER).unwrap_or_default().lines() {
        if let Some(caps) = rx::entry_heading_rx().captures(line) {
            if let Some(id) = caps.get(1) {
                present.push(id.as_str().to_string());
            }
        }
    }
    let unique: BTreeSet<&str> = present.iter().map(String::as_str).collect();

    let missing: Vec<String> = allocated
        .iter()
        .filter(|id| !unique.contains(*id))
        .map(|id| format!("ScrAP-{id}"))
        .collect();
    let mut seen = BTreeSet::new();
    let duplicated: BTreeSet<String> = present
        .iter()
        .filter(|id| !seen.insert((*id).clone()))
        .map(|id| format!("ScrAP-{id}"))
        .collect();
    let added: Vec<&str> = unique
        .iter()
        .filter(|id| !allocated.contains(*id))
        .copied()
        .collect();

    let mut ok = true;
    if !missing.is_empty() {
        ok = fail(
            "allocated number(s) no longer have a '## N.' heading:",
            &missing,
            &[
                "A number is never released. If the entry was merged or superseded, leave a",
                "one-line landing-spot stub under its heading — code and sibling entries",
                "still cite it.",
            ],
        );
    }
    if !duplicated.is_empty() {
        ok = fail(
            "number(s) used by more than one heading:",
            &duplicated.into_iter().collect::<Vec<_>>(),
            &[],
        );
    }
    // INFO, not a failure: a new entry is legitimate work in progress until the commit that
    // carries it also appends to the manifest.
    if !added.is_empty() {
        println!(
            "  INFO — new number(s) not yet in the manifest: {}",
            added.join(" ")
        );
        println!("    Append them to {MANIFEST} in the same commit that adds the entry.");
    }
    if ok {
        return pass();
    }
    false
}

/// Check 10 — a compressed entry keeps its implementation line.
///
/// A stub replaces a full body with a pointer, and the ONE thing it must keep that no
/// external register can ever carry is where THIS project implements the lesson. Drop that
/// line and the entry becomes strictly worse than either register alone: the mechanism is
/// elsewhere and the local answer is gone.
///
/// THE LABEL IS ENUMERATED, NOT GUESSED. The register spells that field five ways
/// (measured, not assumed) — `**Scribobulate**`, with and without a trailing full stop,
/// `**Where Scribobulate implements the fix**` likewise, and one `**Non-core (...)**`
/// variant naming it. A check keyed on one spelling reports a confident absence for every
/// other, and during the compression migration a pass came within one batch of deleting a
/// field it "could not see" for exactly that reason. So match the STEM — and only inside
/// the leading bold run, because a `**Resolution**: … in Scribobulate the zoom provider …`
/// line names the project in PROSE and credited three live entries with an implementation
/// pointer they do not carry.
///
/// Scope: only entries that have already been compressed — one carrying Resolution or Root
/// cause or Lesson is still a full body and is not making a stub's promise.
pub fn stub_keeps_implementation_line(tree: &Tree) -> bool {
    header("10", "a compressed entry keeps its implementation line");
    let mut findings = Vec::new();
    let mut entry: Option<String> = None;
    let mut has_implementation = false;
    let mut is_stub = true;
    let mut has_body = false;

    let mut close = |entry: &Option<String>, is_stub: bool, has_body: bool, has_impl: bool| {
        if let Some(id) = entry {
            if is_stub && has_body && !has_impl {
                findings.push(format!("ScrAP-{id}"));
            }
        }
    };

    for line in tree.text(REGISTER).unwrap_or_default().lines() {
        if let Some(caps) = rx::entry_heading_rx().captures(line) {
            close(&entry, is_stub, has_body, has_implementation);
            entry = caps.get(1).map(|id| id.as_str().to_string());
            has_implementation = false;
            is_stub = true;
            has_body = false;
        } else if line.starts_with("**Symptom") {
            has_body = true;
        } else if line.starts_with("**Resolution")
            || line.starts_with("**Root cause")
            || line.starts_with("**Lesson")
            || line.starts_with("**What was tried")
        {
            // ORDER IS LOAD-BEARING: the stub-disqualifiers are tested BEFORE the
            // implementation-pointer arm, because a `**Resolution**` line can also name the
            // project and would otherwise be consumed as the pointer, leaving a full body
            // classified as a stub.
            is_stub = false;
        } else if let Some(rest) = line.strip_prefix("**") {
            let lead = rest.split('*').next().unwrap_or(rest);
            if lead.contains("Scribobulate") {
                has_implementation = true;
            }
        }
    }
    close(&entry, is_stub, has_body, has_implementation);

    if findings.is_empty() {
        return pass();
    }
    fail(
        "compressed entr(ies) with no implementation line:",
        &findings,
        &[
            "A stub without it points at a lesson and answers nothing locally. If the",
            "entry genuinely has no implementation here (a pure discipline lesson), that",
            "is fine — but say so in the body rather than leaving the field absent.",
        ],
    )
}

/// The growth ratchet's thresholds, in BYTES.
///
/// Lines are the wrong unit and this register proved it: trimming its index cut 72% of that
/// block's bytes while the line count went UP, so a line budget watched the file bloat
/// sideways for months and would not have seen it shrink either. The thresholds are set
/// above the measured state at the time they were written, not at it — a ratchet you trip
/// on the commit that installs it teaches people to raise the number rather than to
/// consolidate.
///
/// **Raised once, 2026-08-27, by operator decision — 520_000/650_000 -> 675_000/700_000.**
/// Recorded because the paragraph above predicted exactly this move and a silent bump would
/// make that warning look unheeded. What actually happened: the ceiling was reached by
/// QA-round entries that had nowhere else to go, and the consolidation the gate asks for was
/// available but not free — see below. The soft limit moved WITH the ceiling on purpose: at
/// 520_000 against a 657_000B file the WARN tier could never fire (the file was already past
/// FAIL), so the two-tier design had quietly collapsed to one tier. A warning that is always
/// on is not a warning.
///
/// **The relief this gate exists to force is still unspent, and a third raise should not
/// happen before it is.** MEASURED 2026-08-27: 29 entries tagged `A` in the index carry
/// ~150_000B of full essays that are migration backlog — their canonical text already lives
/// in the `gtk4-rs` skill, and the routing rule says the body here should be a four-line
/// stub. Six of the largest (ScrAP-193, 238, 252, 258, 259, 268) were spot-checked against
/// the installed skill and confirmed carried there; stubbing just those reclaims ~45_000B.
/// The reason that was not simply done here is that it deletes prose from a tracked register
/// whose only fallback is git history plus a skill that is not installed on every machine
/// this repository lives on — an operator call, not a lint fix.
const REGISTER_WARN: u64 = 675_000;
const REGISTER_FAIL: u64 = 700_000;
const ENTRY_WARN: u64 = 11_000;
const ENTRY_FAIL: u64 = 15_000;

/// Check 11 — register growth, in bytes.
pub fn growth(tree: &Tree) -> bool {
    header("11", "register growth (bytes, not lines)");
    let text = tree.text(REGISTER).unwrap_or_default();
    let register_bytes = normalised_bytes(text);
    let mut failed = false;

    if register_bytes > REGISTER_FAIL {
        fail(
            &format!("register is {register_bytes}B, past the {REGISTER_FAIL}B ceiling"),
            &[],
            &["Consolidate as part of the change that tripped this, not later."],
        );
        failed = true;
    } else if register_bytes > REGISTER_WARN {
        println!("  WARN — register is {register_bytes}B, past the {REGISTER_WARN}B soft limit");
    }

    let sizes = entry_sizes(text);
    let over_warn: Vec<String> = sizes
        .iter()
        .filter(|(_, bytes)| **bytes > ENTRY_WARN)
        .map(|(id, bytes)| format!("ScrAP-{id} {bytes}B"))
        .collect();
    let over_fail: Vec<String> = sizes
        .iter()
        .filter(|(_, bytes)| **bytes > ENTRY_FAIL)
        .map(|(id, bytes)| format!("ScrAP-{id} {bytes}B"))
        .collect();

    if !over_fail.is_empty() {
        fail(
            &format!("entr(ies) past the {ENTRY_FAIL}B per-entry ceiling:"),
            &over_fail,
            &[],
        );
        failed = true;
    } else if !over_warn.is_empty() {
        println!("  WARN — entr(ies) past the {ENTRY_WARN}B per-entry soft limit:");
        for entry in &over_warn {
            println!("    {entry}");
        }
    }

    if !failed && over_warn.is_empty() && register_bytes <= REGISTER_WARN {
        return pass();
    }
    !failed
}

/// Check 13 — every entry body has a row in the table of contents.
///
/// THE CONVERSE OBLIGATION, and it exists because it was violated by the seat that owns this
/// register, on the entry it had just landed: a body that was correct and complete and had
/// no TOC row. SDD principle 7 makes the TOC a FILTER — an agent reads it to decide which
/// bodies to open — so an entry missing from it is invisible to the one path meant to find
/// it while still existing, which is worse than a missing entry because nothing anywhere
/// reads as wrong.
///
/// Nothing else could see it. Check 9 is number immutability, check 10 is the implementation
/// line, checks 2 and 3 prove a cited number HAS a body. None asks whether a body can be
/// FOUND.
///
/// COVERAGE OF THE CONVERSE DIRECTION, mapped so nobody re-derives it: a row left behind
/// after its BODY was deleted is already caught by check 9. The one shape that slips through
/// both is an INVENTED row for a number that was never allocated — assessed and deliberately
/// NOT gated: it needs someone to hand-type a row for an entry that does not exist, and the
/// row is written beside the body.
pub fn body_without_toc_row(tree: &Tree) -> bool {
    header("13", "entry bodies with no TOC row");
    let text = tree.text(REGISTER).unwrap_or_default();
    let rows: BTreeSet<u32> = text
        .lines()
        .filter_map(|line| rx::toc_row_rx().captures(line))
        .filter_map(|caps| caps.get(1)?.as_str().parse().ok())
        .collect();
    let mut findings = Vec::new();
    let mut reported = BTreeSet::new();
    for line in text.lines() {
        let Some(caps) = rx::entry_number_rx().captures(line) else {
            continue;
        };
        let Some(number) = caps.get(1).and_then(|id| id.as_str().parse::<u32>().ok()) else {
            continue;
        };
        if rows.contains(&number) || !reported.insert(number) {
            continue;
        }
        findings.push(format!(
            "ScrAP-{number}  {}",
            line.chars().take(90).collect::<String>()
        ));
    }
    if findings.is_empty() {
        return pass();
    }
    fail(
        "entr(ies) with a body but no row in the table of contents:",
        &findings,
        &[
            "The TOC is how an agent decides which bodies to read (SDD principle 7), so an",
            "entry absent from it is unreachable by the path meant to find it.",
        ],
    )
}

/// The size of a text in bytes AS THE REPOSITORY STORES IT — one newline per line,
/// whatever the working copy materialised.
///
/// This is not pedantry, it is the difference between a gate and a platform split.
/// `.gitattributes` sets `* text=auto`, so a Windows checkout of the register is CRLF and a
/// Linux one is LF; a raw byte count therefore reads ~4,300 bytes larger on Windows for
/// identical content. MEASURED at the time of writing: 645,326B on Linux against 649,617B
/// on Windows, with the ceiling at 650,000. Nothing was failing — and the next few
/// paragraphs added to the register would have failed the gate on Windows alone, for a
/// reason that has nothing to do with growth, in the one gate whose whole purpose is that
/// no platform is the lenient one. The per-entry figures were already line-based and so
/// already immune; this makes the total agree with them.
pub fn normalised_bytes(text: &str) -> u64 {
    text.lines().map(|line| line.len() as u64 + 1).sum()
}

/// Each entry's body size in bytes — the lines below its heading, up to the next one. The
/// heading itself is not counted, so the figure is the body a reader has to get through.
fn entry_sizes(text: &str) -> BTreeMap<String, u64> {
    let mut sizes = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        if let Some(caps) = rx::entry_heading_rx().captures(line) {
            current = caps.get(1).map(|id| id.as_str().to_string());
            if let Some(id) = &current {
                sizes.entry(id.clone()).or_insert(0);
            }
            continue;
        }
        if let Some(id) = &current {
            *sizes.entry(id.clone()).or_insert(0) += line.len() as u64 + 1;
        }
    }
    sizes
}
