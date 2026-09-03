//! The corpus for checks 16 and 17 — the gate on the man-page gate.
//!
//! Same contract as `corpus.rs`: every case calls the extractor THE CHECK calls, never a
//! copy of it, because a corpus over a re-implementation is evidence about the copy.
//!
//! SEPARATE FILE, for two reasons and neither is taste. `corpus.rs` is already twice the
//! 500-line soft limit, and it carries a self-exclusion (`lint::CORPUS_FILE`) it needs
//! because it quotes the citation forms checks 1, 6a and 8 hunt for. Nothing here quotes a
//! citation, so this file is linted like any other source — which is the better default,
//! and worth not giving up by parking these cases next to the exclusion.
//!
//! **The cases that matter are the last one or two in each group.** A corpus of things
//! that obviously work proves nothing; each group below ends on the case that was a real
//! defect or would silently make the check lenient.

use crate::lint::vocab::{
    about_text, config_keys_in_code, config_keys_in_man, roff_plain, theme_keys_in_code,
    theme_keys_in_man, theme_keys_in_schema, Divergence,
};
use std::collections::BTreeSet;

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

// ── Check 16: the theme vocabulary, as the code declares it ───────────────────

/// A `keys!` table in miniature. The last declaration is the one that matters: `rustfmt`
/// puts a long constant's `= "name" : Kind` on its own line, and a pattern requiring the
/// constant and the name together loses exactly those keys — the longest-named ones,
/// which are also the ones a documenter is most likely to miss.
const KEYS_TABLE: &str = r#"
keys! {
    NAME                   = "name"                    : Text,
                             Reach::none("the theme's own label");
    BACKGROUND             = "background"              : Color;
    HEADING_SCALE          = "heading_scale"           : Float  Heading
                             | float(&[2.2, 1.0], SCALE);
    LIST_BULLET_SPRITE     = "list_bullet_sprite"      : Sprite Depth;
    HEADING_BAND_GRADIENT_TO_COLOR
                           = "heading_band_gradient_to_color" : Color Heading,
                             Reach::gated_on("heading_band_color");
}
"#;

#[test]
fn the_key_table_extractor_reads_a_declaration_rustfmt_has_split() {
    assert_eq!(
        theme_keys_in_code(KEYS_TABLE),
        set(&[
            "background",
            "heading_band_gradient_to_color",
            "heading_scale",
            "list_bullet_sprite",
            "name",
        ]),
    );
}

/// Nothing in the file that merely LOOKS like a declaration may be read as one. The last
/// case is a doc comment naming a key, which is ordinary in that file and must not enter
/// the vocabulary — a phantom key would make every document look incomplete.
#[test]
fn the_key_table_extractor_ignores_text_that_is_not_a_declaration() {
    const NOT_DECLARATIONS: &str = r#"
    const SYSTEM_ID: &str = "system";
    let leaf = dir.join("themes.toml");
    /// Falls back to "heading_color" when the level states none.
    "#;
    assert!(theme_keys_in_code(NOT_DECLARATIONS).is_empty());
}

// ── Check 16: the theme vocabulary, as the manual documents it ────────────────

/// A man page in miniature. Three traps, in order: a `.B` line that no `.TP` introduced is
/// running text rather than a definition; a definition in a DIFFERENT `.SH` section is not
/// a theme key; and a `.SS` grouping does not itself define anything.
const MAN_PAGE: &str = r#".TH SCRIBOBULATE 5 "" "" ""
.SH CONFIGURATION FILE
.SS [window]
.TP
.BR width " (integer, default " 900 )
Width in pixels.
.TP
.BR height " (integer, default " 720 )
Height in pixels.
.SS [outline]
.TP
.BR width " (integer, default " 240 )
Sidebar width.
.SH THEME KEYS
.SS Base
.TP
.B background
Colour. Page background.
.B foreground
is described in running text here and defines nothing.
.SS Lists
.TP
.B list_marker_color \fR(tiered)\fP
Colour. Marker ink.
.SH SEE ALSO
.TP
.B scribobulate
Not a key.
"#;

#[test]
fn the_manual_extractor_reads_only_tagged_definitions_in_the_named_section() {
    assert_eq!(
        theme_keys_in_man(MAN_PAGE),
        set(&["background", "list_marker_color"]),
    );
}

/// The config settings are qualified by their `.SS [section]`, and the fixture has `width`
/// under two of them on purpose: unqualified, one documented `width` would answer for both
/// and a page documenting half the settings would pass.
#[test]
fn the_manual_extractor_qualifies_a_config_setting_by_its_section() {
    assert_eq!(
        config_keys_in_man(MAN_PAGE),
        set(&["outline.width", "window.height", "window.width"]),
    );
}

// ── Check 16: the two vocabularies as their own sources declare them ──────────

#[test]
fn the_config_extractor_walks_from_config_into_each_section_struct() {
    const CONFIG_RS: &str = r#"
pub(crate) struct Config {
    pub window: WindowConfig,
    pub outline: OutlineConfig,
}

pub(crate) struct WindowConfig {
    pub width: i32,
    pub height: i32,
}

pub(crate) struct OutlineConfig {
    /// Default width (px) of the outline sidebar pane.
    pub width: i32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self { width: 900, height: 720 }
    }
}
"#;
    assert_eq!(
        config_keys_in_code(CONFIG_RS),
        set(&["outline.width", "window.height", "window.width"]),
    );
}

/// SCHEMA.md states the SUFFIX vocabulary (`_color`, `_sprite`) in the same table shape it
/// states the keys in, above the keys heading. A sweep of the whole file would collect
/// those as keys and then report them as documentation for keys that do not exist.
#[test]
fn the_schema_extractor_starts_at_the_keys_heading_and_not_before() {
    const SCHEMA: &str = r#"
### Key naming

| Suffix | Type | Meaning |
|--------|------|---------|
| `_color` | colour | Every colour-valued key. |
| `_sprite` | sprite path | An image. |

### `[themes.<id>]` keys

#### Base

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `background` | colour | derived | Page background. |
| `list_marker_color` (3) | colour | widget foreground | Marker ink. |
"#;
    assert_eq!(
        theme_keys_in_schema(SCHEMA),
        set(&["background", "list_marker_color"]),
    );
}

// ── Check 16: the report ──────────────────────────────────────────────────────

/// The two directions are separate findings because they are separate defects. A single
/// "these differ" would leave the reader to work out which of the two files to edit.
#[test]
fn a_divergence_names_each_direction_as_its_own_finding() {
    let divergence = Divergence::between(&set(&["kept", "added"]), &set(&["kept", "stale"]));
    assert_eq!(divergence.undocumented, vec!["added".to_string()]);
    assert_eq!(divergence.stale, vec!["stale".to_string()]);

    let findings = divergence.findings("the code", "the manual");
    assert!(findings
        .iter()
        .any(|f| f.contains("added") && f.contains("absent from the manual")));
    assert!(findings
        .iter()
        .any(|f| f.contains("stale") && f.contains("absent from the code")));
    let agreed = Divergence::between(&set(&["same"]), &set(&["same"]));
    assert!(agreed.undocumented.is_empty() && agreed.stale.is_empty());
    assert!(agreed.findings("the code", "the manual").is_empty());
}

// ── Check 17: the About dialog's shape ────────────────────────────────────────

const ABOUT_RS: &str = r#"
            let dialog = AboutDialog::builder()
                .program_name("Scribobulate")
                .website("https://example.test/project")
                .website_label("The project")
                .authors(vec!["(c) 2026 Somebody".to_string()])
                .build();

            dialog.add_credit_section(
                "Bundled components",
                &["A grammar set", "Licensed permissively"],
            );
"#;

#[test]
fn the_about_extractor_reads_each_part_of_the_dialog() {
    let about = about_text(ABOUT_RS);
    assert_eq!(
        about.links,
        vec![
            "https://example.test/project".to_string(),
            "The project".to_string()
        ]
    );
    assert_eq!(about.copyright.as_deref(), Some("(c) 2026 Somebody"));
    assert_eq!(
        about.credits,
        vec![vec![
            "Bundled components".to_string(),
            "A grammar set".to_string(),
            "Licensed permissively".to_string(),
        ]]
    );
    assert!(about.is_well_formed());
    assert_eq!(about.anchors().len(), 6);
}

/// THE MEASURED DEFECT. The well-formedness test was once "the flattened anchor list is not
/// empty", and neutering the website and credit-section extractors left the `authors`
/// literal behind — so the list was non-empty, the guard held its peace, and check 17
/// reported PASS having compared one string out of six. Each part is asserted separately
/// now, and this case is the reason.
#[test]
fn a_dialog_missing_one_part_is_malformed_even_though_another_part_survives() {
    let only_authors = r#".authors(vec!["(c) 2026 Somebody".to_string()])"#;
    let about = about_text(only_authors);
    assert!(
        !about.anchors().is_empty(),
        "the surviving part is still read"
    );
    assert!(
        !about.is_well_formed(),
        "a partial extraction must refuse, not compare a subset"
    );

    let no_credits = ABOUT_RS.replace("add_credit_section", "some_other_call");
    assert!(!about_text(&no_credits).is_well_formed());

    let empty_section = ABOUT_RS.replace(r#""A grammar set", "Licensed permissively""#, "");
    assert!(
        !about_text(&empty_section).is_well_formed(),
        "a credit section with a title and no lines carries no attribution"
    );
}

// ── Check 17: comparing a Rust literal against roff ───────────────────────────

/// The escapes the two pages actually use, and — the case that matters — a paragraph the
/// author has re-wrapped. Whitespace is collapsed on both sides so that re-flowing a
/// paragraph cannot fail the gate: a check that goes red on a cosmetic edit is one its
/// next reader learns to distrust.
#[test]
fn roff_is_rendered_down_far_enough_to_match_the_string_the_dialog_states() {
    let man = ".SS Credits\nSyntax grammars \\(em via two-face\n.br\n\\(co 2026 extollIT\n";
    let rendered = roff_plain(man);
    assert!(rendered.contains(&roff_plain("Syntax grammars \u{2014} via two-face")));
    assert!(rendered.contains(&roff_plain("\u{a9} 2026 extollIT")));

    let wrapped = "Licensed MIT, Apache-2.0,\nBSD-2-Clause, and\nBSD-3-Clause\n";
    assert!(roff_plain(wrapped).contains(&roff_plain(
        "Licensed MIT, Apache-2.0, BSD-2-Clause, and BSD-3-Clause"
    )));

    assert_eq!(roff_plain("\\fBTHIRD\\-PARTY\\fR"), "THIRD-PARTY");
}
