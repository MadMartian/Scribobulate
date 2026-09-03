//! The vocabularies checks 16 and 17 compare, each extracted in exactly ONE place.
//!
//! Same rule, and the same reason, as `patterns.rs`: a corpus that re-implements an
//! extractor is evidence about the corpus. Every function here is called by the check and
//! by the corpus that proves the check discriminates, so the two cannot drift.
//!
//! **What these extractors are, and what they are not.** They read a Rust source file and
//! two roff sources as TEXT. That is not a parser and does not pretend to be one — it
//! works because each of the three declares its vocabulary in a shape that is regular by
//! construction: `keys.rs`'s `keys!` table is one macro line per key, `config.rs`'s
//! sections are plain structs, and a man page tags a definition with `.TP` followed by a
//! `.B`. A restructure that breaks the shape makes the gate FAIL rather than pass quietly
//! — the set comes back short and the check names every key it could not account for,
//! which is the direction a broken extractor must fail in.

use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

/// The `keys!` table's declaration form: `SOME_KEY = "some_key" : Kind …`.
///
/// Anchored on the `= "name" : Kind` half rather than on the constant, deliberately. Two
/// entries in that table are long enough that `rustfmt` puts the constant on its own line
/// (`heading_band_gradient_to_color` and `disclosure_band_gradient_to_color`), so a
/// pattern requiring both halves on one line silently loses exactly the keys whose names
/// are hardest to remember to document.
fn theme_key_decl_rx() -> &'static Regex {
    static RX: OnceLock<Regex> = OnceLock::new();
    rx(
        &RX,
        r#"=\s*"([a-z0-9_]+)"\s*:\s*(Text|Color|Font|Float|Int|Line|Glyph|Sprite)\b"#,
    )
}

/// A `pub <field>: <Type>,` line inside a config struct.
fn struct_field_rx() -> &'static Regex {
    static RX: OnceLock<Regex> = OnceLock::new();
    rx(&RX, r"(?m)^\s*pub ([a-z0-9_]+): ([A-Za-z0-9_]+)")
}

/// `.website("…")`, `.website_label("…")` and the first `.authors(vec!["…"` literal — the
/// About dialog's attribution fields.
fn about_field_rx() -> &'static Regex {
    static RX: OnceLock<Regex> = OnceLock::new();
    rx(
        &RX,
        r#"\.(?:website|website_label)\("([^"]+)"\)|\.authors\(vec!\["([^"]+)""#,
    )
}

fn rx(cell: &'static OnceLock<Regex>, pattern: &'static str) -> &'static Regex {
    cell.get_or_init(|| match Regex::new(pattern) {
        Ok(compiled) => compiled,
        Err(why) => panic!("lint-references: pattern {pattern} does not compile: {why}"),
    })
}

/// Every reading-theme key this build knows, from `src/theme/keys.rs`.
///
/// BARE names only. A key that varies by heading level or list depth is spelled
/// `heading_color_h2` / `list_marker_color_3` in a theme file, but it is DECLARED once and
/// documented once with its suffix rule stated separately — so comparing the levelled
/// spellings would compare a rule against a table rather than a vocabulary against itself.
pub fn theme_keys_in_code(source: &str) -> BTreeSet<String> {
    theme_key_decl_rx()
        .captures_iter(source)
        .filter_map(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
        .collect()
}

/// Every reading-theme key documented in the section 5 man page.
pub fn theme_keys_in_man(man: &str) -> BTreeSet<String> {
    tagged_definitions(man, "THEME KEYS")
        .into_iter()
        .map(|(_group, key)| key)
        .collect()
}

/// Every reading-theme key documented in `sdd/SCHEMA.md`'s per-key tables.
///
/// Scoped to the `[themes.<id>]` keys section and below: the document's earlier tables
/// describe suffixes and resolution order in the same Markdown table shape, and sweeping
/// the whole file would collect `_color` and `_sprite` as if they were keys.
pub fn theme_keys_in_schema(schema: &str) -> BTreeSet<String> {
    let Some(section) = schema.split_once(SCHEMA_KEYS_HEADING) else {
        return BTreeSet::new();
    };
    let (_before, body) = section;
    body.lines()
        .filter_map(|line| {
            let rest = line.trim_start().strip_prefix('|')?.trim_start();
            let name = rest.strip_prefix('`')?.split('`').next()?;
            (!name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
                .then(|| name.to_string())
        })
        .collect()
}

/// The heading that opens SCHEMA.md's per-key tables. A literal because it is a heading in
/// another document, which is the one thing this gate can only match by spelling.
const SCHEMA_KEYS_HEADING: &str = "### `[themes.<id>]` keys";

/// Every `config.toml` setting this build reads, as `section.key`, from `src/config.rs`.
///
/// Qualified by section because the names are not unique on their own — `width` is both
/// `[window]`'s and `[outline]`'s, and an unqualified comparison would call a page that
/// documents one of them complete.
pub fn config_keys_in_code(source: &str) -> BTreeSet<String> {
    // `Config`'s own fields name the sections and the struct each one is spelled by; every
    // other struct in the file is one of those sections.
    let mut bodies: BTreeMap<&str, &str> = BTreeMap::new();
    for (name, body) in struct_bodies(source) {
        bodies.insert(name, body);
    }
    let Some(root) = bodies.get("Config") else {
        return BTreeSet::new();
    };
    let mut keys = BTreeSet::new();
    for caps in struct_field_rx().captures_iter(root) {
        let (Some(section), Some(ty)) = (caps.get(1), caps.get(2)) else {
            continue;
        };
        let Some(body) = bodies.get(ty.as_str()) else {
            continue;
        };
        for field in struct_field_rx().captures_iter(body) {
            if let Some(name) = field.get(1) {
                keys.insert(format!("{}.{}", section.as_str(), name.as_str()));
            }
        }
    }
    keys
}

/// Every `config.toml` setting documented in the section 5 man page, as `section.key`.
///
/// The section is the page's own `.SS [window]` grouping, with the brackets that make it
/// read as a TOML table header stripped back off.
pub fn config_keys_in_man(man: &str) -> BTreeSet<String> {
    tagged_definitions(man, "CONFIGURATION FILE")
        .into_iter()
        .filter_map(|(group, key)| {
            let section = group?;
            let section = section.strip_prefix('[')?.strip_suffix(']')?;
            Some(format!("{section}.{key}"))
        })
        .collect()
}

/// The About dialog's attribution strings, from `src/app/appactions.rs`.
///
/// These are a LICENCE OBLIGATION rather than decoration (POLICY, Third-party
/// attribution): the running application tells every user the notices travel with the
/// distribution, and the man page is the copy a reader with no display gets. Extracted
/// from the dialog rather than restated here, so this gate holds a comparison and not a
/// third opinion.
///
/// **Returned BY PART rather than as one flat list, and that is load-bearing.** It was a
/// flat `Vec<String>` guarded by "not empty", and MEASURED: neutering both the website and
/// the credit-section extractors left the `authors` literal behind, so the list was still
/// non-empty and the check reported PASS with two thirds of its input silently gone. A
/// guard that fires only when EVERY extractor dies is a guard against nothing that
/// happens. The caller asserts each part is present instead.
pub struct AboutText {
    /// The `.website(…)` URL and its `.website_label(…)`.
    pub links: Vec<String>,
    /// The `.authors(vec![…])` line — this project spends it on the copyright holder.
    pub copyright: Option<String>,
    /// One entry per `add_credit_section(…)`: its title followed by its lines.
    pub credits: Vec<Vec<String>>,
}

impl AboutText {
    /// Every string a reader of the dialog sees, in one list, for the comparison itself.
    pub fn anchors(&self) -> Vec<String> {
        let mut all = self.links.clone();
        all.extend(self.copyright.clone());
        all.extend(self.credits.iter().flatten().cloned());
        all
    }

    /// Whether the dialog still has the SHAPE this check knows how to read: a link, a
    /// copyright line, and at least one credit section that is not empty.
    ///
    /// A dialog that legitimately loses one of these makes the gate REFUSE rather than
    /// pass — the right direction, because the extractor then needs a human, and a
    /// refusal says so where a green run would not.
    pub fn is_well_formed(&self) -> bool {
        !self.links.is_empty()
            && self.copyright.is_some()
            && !self.credits.is_empty()
            && self.credits.iter().all(|section| section.len() > 1)
    }
}

pub fn about_text(source: &str) -> AboutText {
    let mut links = Vec::new();
    let mut copyright = None;
    for caps in about_field_rx().captures_iter(source) {
        if let Some(link) = caps.get(1) {
            links.push(link.as_str().to_string());
        } else if let Some(holder) = caps.get(2) {
            copyright.get_or_insert_with(|| holder.as_str().to_string());
        }
    }
    AboutText {
        links,
        copyright,
        credits: credit_sections(source),
    }
}

/// Every `add_credit_section(…)` call's string literals — its title, then its lines.
fn credit_sections(source: &str) -> Vec<Vec<String>> {
    let mut sections = Vec::new();
    for (at, _) in source.match_indices(CREDIT_SECTION_CALL) {
        let after = &source[at + CREDIT_SECTION_CALL.len()..];
        // The call ends at the first `);` that closes it. Every literal inside is plain —
        // a `<…>` in this text would be mis-parsed by GTK as a mailto: link, so the
        // dialog's own rules already forbid the characters that would need escaping.
        let Some(end) = after.find(");") else {
            continue;
        };
        let mut rest = &after[..end];
        let mut lines = Vec::new();
        while let Some(open) = rest.find('"') {
            let tail = &rest[open + 1..];
            let Some(close) = tail.find('"') else {
                break;
            };
            lines.push(tail[..close].to_string());
            rest = &tail[close + 1..];
        }
        sections.push(lines);
    }
    sections
}

const CREDIT_SECTION_CALL: &str = "add_credit_section(";

/// A man page's definition tags inside one `.SH` section: `(the enclosing .SS, the term)`.
///
/// The shape it reads is the one every man page uses for a definition list — `.TP`, then a
/// `.B`/`.BR` line whose first word is the term. Nothing is filtered on the way out: a
/// `.TP` whose term is not a key comes back as a term the code cannot account for, and the
/// check reports it. Dropping unrecognised terms would make the page's own structure the
/// thing that decides what is checked.
fn tagged_definitions(man: &str, section: &str) -> Vec<(Option<String>, String)> {
    let mut found = Vec::new();
    let mut inside = false;
    let mut group: Option<String> = None;
    let mut tagged = false;
    for line in man.lines() {
        if let Some(name) = line.strip_prefix(".SH ") {
            inside = name.trim() == section;
            group = None;
            tagged = false;
            continue;
        }
        if !inside {
            continue;
        }
        if let Some(name) = line.strip_prefix(".SS ") {
            group = Some(name.trim().to_string());
            tagged = false;
            continue;
        }
        if line.trim_end() == ".TP" {
            tagged = true;
            continue;
        }
        if tagged {
            if let Some(term) = definition_term(line) {
                found.push((group.clone(), term));
            }
            tagged = false;
        }
    }
    found
}

/// The term a `.B`/`.BR` line defines: its first word, with font escapes removed.
fn definition_term(line: &str) -> Option<String> {
    let rest = line
        .strip_prefix(".BR ")
        .or_else(|| line.strip_prefix(".B "))?;
    let word = rest.split_whitespace().next()?;
    let word = roff_plain(word);
    (!word.is_empty()).then_some(word)
}

/// roff source rendered down to the text a reader sees, flattened to one line.
///
/// Only the escapes these two pages use are decoded — this is not a roff implementation,
/// and it does not need to be: its whole job is to let a comparison against a Rust string
/// literal survive the spellings roff requires. Whitespace is collapsed LAST so that an
/// anchor found on one source line still matches after someone re-wraps the paragraph;
/// without that, the gate would fail on a purely cosmetic edit and teach its next reader
/// to distrust it.
pub fn roff_plain(text: &str) -> String {
    let decoded = text
        .replace("\\(em", "\u{2014}")
        .replace("\\(en", "\u{2013}")
        .replace("\\(co", "\u{a9}")
        .replace("\\(bu", "\u{2022}")
        .replace("\\(aq", "'")
        .replace("\\fB", "")
        .replace("\\fI", "")
        .replace("\\fR", "")
        .replace("\\fP", "")
        .replace("\\-", "-")
        .replace("\\&", "");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `struct <Name> { … }` bodies, by name.
///
/// Brace-counted rather than regex-matched: a struct body is nested (attribute macros,
/// generic bounds), and a non-greedy `\{(.*?)\}` stops at the first inner brace it meets.
fn struct_bodies(source: &str) -> Vec<(&str, &str)> {
    let mut bodies = Vec::new();
    for (at, _) in source.match_indices("struct ") {
        let after = &source[at + "struct ".len()..];
        let Some(name_end) = after.find(|c: char| !c.is_alphanumeric() && c != '_') else {
            continue;
        };
        let name = &after[..name_end];
        let Some(open) = after.find('{') else {
            continue;
        };
        // A `struct X;` or `struct X(…)` has no brace body of its own, and the `{` found
        // above belongs to whatever follows it in the file.
        if after[name_end..open].contains(';') {
            continue;
        }
        let mut depth = 0usize;
        let mut close = None;
        for (offset, ch) in after[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(open + offset);
                        break;
                    }
                }
                _ => {}
            }
        }
        if let Some(close) = close {
            bodies.push((name, &after[open + 1..close]));
        }
    }
    bodies
}

/// The two directions a vocabulary comparison can fail, reported separately.
///
/// Separately because they are different defects with different fixes: a key in the code
/// and not the document is undocumented behaviour, a key in the document and not the code
/// is a promise the application no longer keeps. A single "these differ" would leave the
/// reader to work out which.
pub struct Divergence {
    pub undocumented: Vec<String>,
    pub stale: Vec<String>,
}

impl Divergence {
    pub fn between(code: &BTreeSet<String>, document: &BTreeSet<String>) -> Self {
        Self {
            undocumented: code.difference(document).cloned().collect(),
            stale: document.difference(code).cloned().collect(),
        }
    }

    /// The findings, already worded for the report.
    pub fn findings(&self, code_site: &str, document: &str) -> Vec<String> {
        let mut out: Vec<String> = self
            .undocumented
            .iter()
            .map(|key| format!("{key} — in {code_site}, absent from {document}"))
            .collect();
        out.extend(
            self.stale
                .iter()
                .map(|key| format!("{key} — in {document}, absent from {code_site}")),
        );
        out
    }
}
