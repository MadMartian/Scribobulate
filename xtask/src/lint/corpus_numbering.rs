//! The corpus for check 18 — the gate on the entry-numbering gate.
//!
//! Same contract as `corpus.rs` and `corpus_manpage.rs`: every case calls the extractor
//! THE CHECK calls, so a corpus cannot pass over a copy the gate does not run.
//!
//! The cases that matter are the near-misses. This check's whole risk is a FALSE positive
//! — it reports a frozen ID as duplicated, someone renumbers a rubric that was fine, and
//! every citation to it across three documents breaks. So most of what is pinned below is
//! text that must NOT be read as a duplicate.

use crate::lint::checks::register::{duplicate_numbers, numbered_entries, EntryKind};

/// A TDD extract with the shapes that actually appear in that file: plain numbers, a
/// letter suffix, and the middle-dot form this project uses for its a11y rubrics. The
/// last two headings are the duplicate.
const TDD: &str = r"# Test-Driven Design

## 7. Window & layout

### 7.0b An early rubric
- **Given** something

### 7.2 Layout persists across sessions
- **Given** something

### 2.2a Table header row is visually distinguished
- **Given** something

### 2.2·a11y Text views use a screen-reader-safe wrap mode
- **Given** something

### Key naming
Prose heading, not a rubric.

### 7.21 Every install route delivers the same payload
- **Given** something

### 7.21 A freshly opened document puts the working position at its beginning
- **Given** something
";

#[test]
fn a_rubric_number_used_twice_is_reported_with_every_line_that_uses_it() {
    let entries = numbered_entries(TDD, EntryKind::Rubric);
    let duplicates = duplicate_numbers(&entries);
    assert_eq!(duplicates.len(), 1, "found: {duplicates:?}");
    let (number, lines) = &duplicates[0];
    assert_eq!(number, "7.21");
    assert_eq!(lines.len(), 2);
    // Reported by LINE, because "7.21 is duplicated" leaves the reader to find both.
    assert!(lines[0] < lines[1]);
}

/// THE FALSE-POSITIVE CASES, and the reason the number is taken whole rather than reduced
/// to its numeric stem. `2.2`, `2.2a` and `2.2·a11y` are three entries in that file, and a
/// check that stemmed them would demand the renumbering of two rubrics that are correct —
/// across `sdd/`, `tests/` and `src/`, since all three are cited.
#[test]
fn a_suffixed_number_is_its_own_entry_and_not_a_duplicate_of_its_stem() {
    let text = r"### 2.2 Tables
### 2.2a Table header row is visually distinguished
### 2.2·a11y Text views use a screen-reader-safe wrap mode
### 2.20 A single source line break renders as a line break
";
    let entries = numbered_entries(text, EntryKind::Rubric);
    assert_eq!(entries.len(), 4, "extracted: {entries:?}");
    assert!(
        duplicate_numbers(&entries).is_empty(),
        "2.2, 2.2a, 2.2\u{b7}a11y and 2.20 are four entries, not one repeated"
    );
}

/// A heading that is not a rubric must not enter the set at all. If prose headings were
/// collected, two `### Notes` sections in one document would be reported as a duplicated
/// entry number, which is not a thing that exists.
#[test]
fn prose_headings_and_deeper_levels_are_not_numbered_entries() {
    let text = r"### Key naming
### Search path
#### 2.2 A deeper heading is not a rubric
## 7. Window & layout
### 7.1 A real one
";
    let entries = numbered_entries(text, EntryKind::Rubric);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].1, "7.1");
}

/// The manual-test plan's shape is different — a checklist item, its number in bold — and
/// it is read by the same predicate under a different `EntryKind`. The `m`/`s`/letter
/// suffixes it uses heavily must stay distinct here too.
#[test]
fn manual_test_items_are_read_from_their_own_shape() {
    let text = r"- [ ] **7.20** A tab opened into a full strip lands after its neighbour
- [ ] **7.21m** macOS: both manual pages install and resolve by name (TDD 7.21)
- [ ] **7.22** A freshly opened document puts the caret at its start (TDD 7.22)
- [ ] An unnumbered checklist item, which some sections do use
- [ ] **8.2m** macOS launch paths
- [ ] **8.2s** Another variant
";
    let entries = numbered_entries(text, EntryKind::Item);
    let numbers: Vec<&str> = entries.iter().map(|(_, id)| id.as_str()).collect();
    assert_eq!(numbers, ["7.20", "7.21m", "7.22", "8.2m", "8.2s"]);
    assert!(duplicate_numbers(&entries).is_empty());
}

/// The two shapes must not read each other's file. A rubric heading is not an item and an
/// item is not a heading; if either predicate matched both, one document's numbers would
/// be pooled with the other's and a `7.21` rubric would "duplicate" a `7.21` item — which
/// is legitimate and extremely common, since an item traces to the rubric it shares a
/// number with.
#[test]
fn one_shape_does_not_read_the_other_documents_entries() {
    let rubric = "### 7.21 Every install route delivers the same payload\n";
    let item = "- [ ] **7.21** A check that traces to it (TDD 7.21)\n";
    assert!(numbered_entries(rubric, EntryKind::Item).is_empty());
    assert!(numbered_entries(item, EntryKind::Rubric).is_empty());
}
