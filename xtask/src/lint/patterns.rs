//! The patterns and predicates the checks are built from, each defined ONCE so a corpus
//! drives the program the gate runs rather than a copy of it.
//!
//! That rule is not stylistic. The PowerShell port this crate replaced had a self-test
//! that asserted against its own re-implementation of check 6a's extraction, so dropping
//! `-CaseSensitive` from the real call site left the corpus GREEN while the gate failed on
//! the tree. A corpus over a pattern is evidence about a PATTERN; only a corpus over the
//! predicate is evidence about the CHECK. Every corpus in `corpus.rs` calls into this
//! module.

use regex::Regex;
use std::sync::OnceLock;

/// The citation forms check 1 must catch.
///
/// `ISSUES` is matched case-SENSITIVELY: a case-insensitive match hits ordinary prose
/// ("issues a queue_draw"), which is a real false positive the PowerShell port hit because
/// `Select-String` defaults the other way. The trailing `\b` after `[A-Z]` is what keeps
/// `sdd/ISSUES.md TOC` from matching — an ID is a single letter standing alone, not the
/// first letter of a word.
///
/// The second alternative catches the `#`-sigil form (a `#` then the letter), which the
/// single-letter branch cannot: `#` is not a separator it accepts, and a MULTI-letter
/// ID (this register has used `WW`/`ZZ`/`BBB`) never satisfies `[A-Z]\b`. Three such
/// references sat in `tests/MANUAL-TEST.md`, all naming issues long since resolved —
/// dangling exactly as SDD principle 6 predicts — while this gate passed. A multi-letter ID
/// is only recognised BEHIND the `#`, because bare uppercase runs after `ISSUES` are
/// ordinary prose (`ISSUES.md TOC`, `ISSUES.md IDs`).
///
/// The connecting word is `[a-z]+` — ANY lowercase word, not an enumeration. It was
/// `(entry )?`, the one noun the pattern's author happened to write, and an `ISSUES.md item
/// <letter>` citation walked through it into `src/`, in the very commit that rewrote the
/// entry it named. Enumerating the vocabulary of a free-text citation is a losing game, so
/// match the SHAPE instead: `ISSUES`, a connector, a lone capital.
///
/// The third alternative is the TITLE form, and it is here because check 1 reported PASS
/// over a live violation: `probes/native-chooser-rss.m` cited an entry by its QUOTED TITLE
/// rather than its letter, and a pattern hunting letter IDs cannot see that. A title
/// pointer dangles exactly as an ID pointer does.
pub fn issues_rx() -> &'static Regex {
    static RX: OnceLock<Regex> = OnceLock::new();
    rx(
        &RX,
        r#"\bISSUES(\.md)?([ .:_-]*([a-z]+ )?[A-Z]\b|[ .:_-]*#[A-Z]+\b|[ .:_-]*"[^"]+")"#,
    )
}

/// The document-path forms check 6a must catch.
///
/// Pinned beside `issues_rx` and corpus-tested for the same reason: check 6 passed clean
/// over 21 live danglers because its pattern required a `.md` the citations did not carry,
/// and nothing proved otherwise. The `.md` alternative is deliberately FIRST — this crate's
/// engine takes leftmost-first, and that ordering is what makes a full plan FILENAME match
/// whole rather than being truncated to its topic by the bare alternative.
pub fn path_rx() -> &'static Regex {
    static RX: OnceLock<Regex> = OnceLock::new();
    rx(
        &RX,
        r"((sdd|tests|packaging|gtktest|scripts|data|src)/[A-Za-z0-9._/-]+\.md|\bPLAN\.[A-Za-z0-9._-]+\.md|\bPLAN\.[A-Za-z0-9_-]+)",
    )
}

/// Markdown link targets pointing at a `.md`, for check 6b.
pub fn md_link_rx() -> &'static Regex {
    static RX: OnceLock<Regex> = OnceLock::new();
    rx(&RX, r"\]\(([^)# ]+\.md)(#[^) ]*)?\)")
}

/// Inline code spans, stripped before check 6b matches: a backticked
/// `[architecture](TECH.md)` in README.md is an example OF link syntax, not a link. It was
/// this check's only false positive.
pub fn code_span_rx() -> &'static Regex {
    static RX: OnceLock<Regex> = OnceLock::new();
    rx(&RX, r"`[^`]*`")
}

/// Check 8's candidate pattern. `[^a-zA-Z-]` excludes a hyphen, so `Scr-AP-9` and
/// identifier fragments like `x-AP-9` do not match; `SOAP-12` is excluded by the alpha
/// class; and `ScrAP-9` is excluded by construction, since `r` is in the class.
fn bare_ap_rx() -> &'static Regex {
    static RX: OnceLock<Regex> = OnceLock::new();
    rx(&RX, r"(^|[^a-zA-Z-])AP-[0-9]+")
}

/// The one legal unresolvable citation form: the `gtk4-rs` skill's. Checkable for FORM
/// only — the skill need not be installed — which is the whole point: legalising it makes
/// the unverifiable set ENUMERABLE rather than verifiable.
fn legal_ap_rx() -> &'static Regex {
    static RX: OnceLock<Regex> = OnceLock::new();
    rx(&RX, r"GTK4Rs/AP-[0-9]+")
}

/// The reserved Win32 device names, in any path component, with or without an extension.
///
/// `COM` TAKES `[1-9]` AND `LPT` TAKES `[0-9]`. The asymmetry reads like a typo and is not:
/// it is git's own predicate. The two shell ports spelled it this way, this crate inherited
/// the spelling, the macOS seat read the two corpus entries as a contradiction, and the
/// Linux seat resolved it toward strict on the documentation's authority — reasonably, since
/// Microsoft's current "Naming Files, Paths, and Namespaces" lists `COM0`-`COM9` as reserved.
/// The documentation is not what blocks a clone.
///
/// MEASURED by the Windows seat, 2026-08-23, Windows 10 19045 / NTFS / git 2.49.0.windows.1,
/// `core.protectNTFS` at its compiled default. Each path was planted with
/// `git -c core.protectNTFS=false update-index --add --cacheinfo` and checked out into a
/// FRESH tree, because a shell-created file and a checkout of a tracked path are different
/// code paths and only the second is the hazard:
///
/// | tracked path                                | `git checkout`                            |
/// |---------------------------------------------|-------------------------------------------|
/// | `com0` `COM0` `com0.txt` `src/com0/x.rs`    | exit 0, file lands, tree clean            |
/// | `com1`..`com9`, `lpt1`..`lpt9`              | `error: invalid path`, WHOLE tree refused |
/// | `lpt0` `LPT0` `lpt0.md` `src/lpt0/x.rs`     | `error: invalid path`, WHOLE tree refused |
/// | `com10`                                     | exit 0, file lands                        |
/// | `con` `nul` `aux` `prn`                     | `error: invalid path`, WHOLE tree refused |
///
/// TWO TRAPS INSIDE THAT MEASUREMENT, and they point in opposite directions. git is STRICTER
/// than the OS for `lpt0`: `New-Item lpt0` creates a real file that `git checkout` will not
/// produce — so the shell probe this rule was nearly relaxed on gives the WRONG answer for
/// the one name where wrong is expensive. And git is LOOSER than the documentation for
/// `com0`. Probing either source alone gets one of the pair wrong. (A third layer disagrees
/// with both: PowerShell's `Test-Path` reports `False` for a `com0`/`lpt0` file that
/// `[System.IO.File]::Exists` reports `True` and that is genuinely on disk.)
///
/// TWO NEIGHBOURING HAZARDS, both now measured, NEITHER covered by this pattern:
/// * The superscripts (`COM¹` `COM²` `COM³` `LPT¹`) break a checkout one layer further out
///   and one degree weaker: git ACCEPTS the path and the OS refuses the create
///   (`error: unable to create file COM¹: No such file or directory`). The clone exits
///   non-zero and the rest of the tree still lands, so it is a broken checkout rather than a
///   blocked one — a different severity from every row above, which is why folding it in
///   here would misdescribe what this check reports.
/// * A literal backslash in a tracked path (`a\b.txt`) is refused exactly like the rows
///   above: `error: invalid path`, whole tree, nothing applied at all. It was expected to be
///   a silent-divergence class — the file landing somewhere else — and it is not. It belongs
///   in this check's class, as its own rule rather than inside a device-name pattern.
fn device_name_rx() -> &'static Regex {
    static RX: OnceLock<Regex> = OnceLock::new();
    rx(&RX, r"(?i)^(CON|PRN|AUX|NUL|COM[1-9]|LPT[0-9])(\..*)?$")
}

/// A reverse-DNS identifier ending in `scribobulate`, for check 7's foreign-ID sweep.
pub fn reverse_dns_rx() -> &'static Regex {
    static RX: OnceLock<Regex> = OnceLock::new();
    rx(&RX, r"\b[a-z0-9]+\.[a-z0-9]+\.scribobulate\b")
}

/// A `## N.` or `## 23a.` entry heading in `sdd/ANTI-PATTERNS.md` — the ALLOCATION form,
/// used by the number-immutability and growth checks, which are about the register's own
/// bookkeeping and so must see a lettered sub-entry as its own allocation.
pub fn entry_heading_rx() -> &'static Regex {
    static RX: OnceLock<Regex> = OnceLock::new();
    rx(&RX, r"^##[[:space:]]+([0-9]+[a-z]?)\.")
}

/// The same heading, digits only — the CITATION form, used by the checks that resolve a
/// `ScrAP-N` and by the TOC-row check. The distinction is deliberate and load-bearing: a
/// citation is always `ScrAP-<digits>`, so a lettered sub-entry defines no citable number
/// and its TOC row (`| 23a |`) is not a row this gate can key on. Merging the two would
/// silently make `## 23a.` answer for `ScrAP-23`.
pub fn entry_number_rx() -> &'static Regex {
    static RX: OnceLock<Regex> = OnceLock::new();
    rx(&RX, r"^##[[:space:]]+([0-9]+)\.")
}

/// A table-of-contents row's leading `| N |` cell. Matched on the number cell only —
/// matching the title too would make this a second place the title is written down.
pub fn toc_row_rx() -> &'static Regex {
    static RX: OnceLock<Regex> = OnceLock::new();
    rx(&RX, r"^\|[[:space:]]*([0-9]+)[[:space:]]*\|")
}

/// A `ScrAP-N` citation.
pub fn scrap_rx() -> &'static Regex {
    static RX: OnceLock<Regex> = OnceLock::new();
    rx(&RX, r"ScrAP-([0-9]+)")
}

/// A plain `mod x;` declaration, for check 4. A `#[cfg]` on the preceding line is ignored,
/// which is what makes `suite_registry` (cfg-gated in the lib, unconditional in the suite
/// root) and the `#[cfg(unix)]`/`#[cfg(windows)]` modules compare equal rather than
/// reading as drift.
pub fn mod_decl_rx() -> &'static Regex {
    static RX: OnceLock<Regex> = OnceLock::new();
    rx(&RX, r"^[[:space:]]*(pub\(crate\) )?mod ([a-z_0-9]+);")
}

/// The canonical application ID, read from `src/icons.rs`'s `Icon::App` arm.
pub fn app_id_rx() -> &'static Regex {
    static RX: OnceLock<Regex> = OnceLock::new();
    rx(&RX, r#"Icon::App => "([^"]+)""#)
}

/// Compile once. A pattern in this file is a literal, so a compile failure is a programmer
/// error caught by the first test that touches it — never a runtime state to degrade from.
fn rx(cell: &'static OnceLock<Regex>, pattern: &'static str) -> &'static Regex {
    cell.get_or_init(|| match Regex::new(pattern) {
        Ok(compiled) => compiled,
        Err(why) => panic!("lint-references: pattern {pattern} does not compile: {why}"),
    })
}

/// Check 8's predicate AND its report, in one function, so the two cannot disagree about
/// what was found.
///
/// The rule is "an `AP-N` not preceded by `Scr` or by `GTK4Rs/`", which is a lookbehind —
/// unavailable in this engine and in POSIX ERE alike, and the reason both shell ports were
/// two-stage too. So: delete every LEGAL citation from the line, then look for a candidate
/// in what is left. A line carrying both forms — `GTK4Rs/AP-N / AP-M` — is thereby reported
/// for the SECOND citation only, which is the correct answer and the one a single regex
/// cannot give. (Spelling that example with real numbers would make THIS file a violation
/// of the check it implements, which is why the corpus lives in `corpus.rs` and that one
/// path is the gate's only self-exclusion.)
///
/// Returns the sorted, deduplicated `AP-N` tokens that are bare; empty means the line is
/// clean.
pub fn bare_ap_citations(line: &str) -> Vec<String> {
    let stripped = legal_ap_rx().replace_all(line, "");
    let mut found: Vec<String> = bare_ap_rx()
        .find_iter(&stripped)
        // The candidate match may carry a leading separator character; the citation is the
        // `AP-N` inside it.
        .filter_map(|m| {
            m.as_str()
                .find("AP-")
                .map(|at| m.as_str()[at..].to_string())
        })
        .collect();
    found.sort();
    found.dedup();
    found
}

/// Check 12's predicate: is this a tracked path Windows refuses to check out?
///
/// `< > : " | ? *`, a control character, a trailing dot or space, or a reserved device name
/// in any component. One such path makes `git checkout` refuse the ENTIRE tree — not that
/// file, the whole tree — blocking every Windows clone and anyone bisecting through the
/// commit.
///
/// The newline case is first and separate because the shell ports had to route the rest
/// through `grep`, which is line-oriented: a `\n` inside a path is a SEPARATOR there and
/// never content, so the control-character clause silently lost its most likely member.
/// This implementation reads paths from `git ls-files -z` whole, so the case is ordinary —
/// but it stays spelled out because the corpus that pins it is the evidence the class is
/// covered.
pub fn win_illegal_path(path: &str) -> bool {
    if path.contains('\n') {
        return true;
    }
    if path.chars().any(|c| {
        matches!(
            c,
            '<' | '>' | ':' | '"' | '|' | '?' | '*' | '\u{1}'..='\u{1f}'
        )
    }) {
        return true;
    }
    // A LITERAL BACKSLASH, which is its own rule rather than a member of the set above: it
    // is not an illegal CHARACTER on Win32, it is the separator, and the expectation here
    // was that a tracked `a\b.txt` would land somewhere else silently — a divergence class
    // this check does not cover. MEASURED by the Windows seat and that premise is refuted:
    // git refuses it exactly like a reserved name, `error: invalid path 'a\b.txt'`, whole
    // tree, nothing applied (verified by a clone directory containing only `.git`). Same
    // class and same blast radius as every other rule here, so it belongs here.
    if path.contains('\\') {
        return true;
    }
    // PER COMPONENT. `/` is the separator in everything this gate sees (`git ls-files -z`),
    // so splitting on it is exact rather than a guess.
    path.split('/').any(|segment| {
        segment.ends_with('.') || segment.ends_with(' ') || device_name_rx().is_match(segment)
    })
}

/// One flagged paragraph from the check-5b scan.
pub struct ProseHit {
    pub file: String,
    pub line: usize,
    pub text: String,
}

/// Check 5b's program: `#[gtk::test]` PRESCRIBED in prose, without the replacement named
/// anywhere in the same paragraph.
///
/// THE UNIT IS THE PARAGRAPH, NOT THE LINE. A first attempt flagged any line naming the
/// attribute with no `gtktest` within two lines, and it failed on three CORRECT lines — the
/// sentences explaining WHY the attribute is banned sit five to nine lines below the one
/// that names the replacement. Widening the window is a fudge; the honest unit is the
/// blank-line-delimited block, because "names the ban without ever naming the replacement"
/// is precisely what distinguishes an INSTRUCTION from an EXPLANATION, and it does not
/// depend on how the author happened to wrap their prose.
///
/// The FILE BOUNDARY is where the awk original had two live bugs, both found by the Windows
/// seat and both invisible to a single-file corpus (ScrAP-226): a pending paragraph was
/// reported with the NEXT file's name, and its state carried across, so a later file
/// mentioning `gtktest` could silently swallow an earlier file's real prescription. Taking
/// a file at a time and flushing at its end makes both unrepresentable rather than fixed —
/// which is why the multi-file corpus in `corpus.rs` stays: it is the evidence the shape
/// still holds.
pub fn prose_prescriptions(files: &[(String, String)]) -> Vec<ProseHit> {
    let mut hits = Vec::new();
    for (name, text) in files {
        let mut pending: Option<(usize, String)> = None;
        let mut replacement_named = false;
        let mut flush = |pending: &mut Option<(usize, String)>, replacement_named: &mut bool| {
            if let Some((line, content)) = pending.take() {
                if !*replacement_named {
                    hits.push(ProseHit {
                        file: name.clone(),
                        line,
                        text: content,
                    });
                }
            }
            *replacement_named = false;
        };
        for (index, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                flush(&mut pending, &mut replacement_named);
                continue;
            }
            if pending.is_none() && line.contains("#[gtk::test]") {
                pending = Some((index + 1, line.to_string()));
            }
            if line.contains("gtktest") {
                replacement_named = true;
            }
        }
        flush(&mut pending, &mut replacement_named);
    }
    hits
}
