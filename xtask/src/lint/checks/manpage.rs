//! The man-page checks: 16 and 17. Both are about the installed manuals still describing
//! the program that is actually built.
//!
//! WHY A GATE AND NOT A REVIEW HABIT. A man page is the one document nobody on the project
//! reads: it is written for a stranger with a terminal, and every developer here already
//! knows what the program does. So a key added to `src/theme/keys.rs` breaks nothing, fails
//! no test, and leaves a manual that is wrong in the direction that reads as complete —
//! ScrAP-123's species exactly, one level up (a fact restated in prose, with nothing
//! comparing it to its source). The manual pages were generated from a heredoc precisely to
//! dodge this, and dodged it by carrying almost no content; they carry content now.

use super::{fail, header, pass};
use crate::lint::vocab;
use crate::lint::Tree;

const MAN1: &str = "packaging/man/scribobulate.1";
const MAN5: &str = "packaging/man/scribobulate.5";
const KEYS: &str = "src/theme/keys.rs";
const CONFIG: &str = "src/config.rs";
const SCHEMA: &str = "sdd/SCHEMA.md";
const ABOUT: &str = "src/app/appactions.rs";

/// Check 16 — the theme and configuration vocabularies agree across the code that
/// implements them, the manual that documents them, and SCHEMA.md that specifies them.
///
/// THREE COPIES OF ONE VOCABULARY, which is two more than anyone wants, and the reason
/// each exists is different: `keys.rs` declares it, SCHEMA.md is what an agent or
/// contributor reads, the man page is what a user reads. Merging them was considered and
/// rejected — SCHEMA carries GTK-internal rationale and register citations that have no
/// business in a user's manual, and generating the manual from SCHEMA would put that
/// content one careless edit away from shipping. So they stay three, and this compares
/// them.
///
/// **WHAT IT CANNOT DO — read this before trusting a PASS.** It compares NAMES. A key
/// whose default changed, whose clamp range moved, or whose meaning was inverted passes
/// untouched, in all three documents. The comparison is against the declaration, not
/// against behaviour, and no textual gate can be otherwise. The residue is a review
/// obligation: when you change what a key MEANS, re-read its paragraph in both documents.
pub fn documented_vocabulary(tree: &Tree) -> bool {
    header(
        "16",
        "theme and config keys documented in the manual (and in SCHEMA)",
    );
    let (Some(keys), Some(config), Some(man), Some(schema)) = (
        tree.text(KEYS),
        tree.text(CONFIG),
        tree.text(MAN5),
        tree.text(SCHEMA),
    ) else {
        // Unreachable through `run`, which refuses to start without these — kept because
        // a check that silently compares nothing against nothing is the failure mode the
        // REQUIRED list exists to prevent, and a `let else` that returns PASS would be
        // exactly that.
        return fail(
            "an input this check reads is missing from the scan set",
            &[format!(
                "expected all of: {KEYS}, {CONFIG}, {MAN5}, {SCHEMA}"
            )],
            &["Refusing to compare vocabularies that were not read."],
        );
    };

    let declared = vocab::theme_keys_in_code(keys);
    if declared.is_empty() {
        return fail(
            &format!("no theme keys were extracted from {KEYS}"),
            &[],
            &[
                "The `keys!` table's declaration form has changed and this check now reads",
                "an empty vocabulary, against which every document is complete.",
                "Fix the extractor in xtask/src/lint/vocab.rs before trusting a PASS.",
            ],
        );
    }

    let mut findings = Vec::new();
    let against_man = vocab::Divergence::between(&declared, &vocab::theme_keys_in_man(man));
    findings.extend(against_man.findings(KEYS, MAN5));
    let against_schema =
        vocab::Divergence::between(&declared, &vocab::theme_keys_in_schema(schema));
    findings.extend(against_schema.findings(KEYS, SCHEMA));

    let settings = vocab::config_keys_in_code(config);
    if settings.is_empty() {
        return fail(
            &format!("no config settings were extracted from {CONFIG}"),
            &[],
            &["As above: an empty vocabulary makes every document look complete."],
        );
    }
    let against_config = vocab::Divergence::between(&settings, &vocab::config_keys_in_man(man));
    findings.extend(against_config.findings(CONFIG, MAN5));

    if findings.is_empty() {
        return pass();
    }
    fail(
        "a documented vocabulary does not match the one the code declares",
        &findings,
        &[
            "A key in the code and not the manual is undocumented behaviour; a key in the",
            "manual and not the code is a promise the application no longer keeps.",
            "Theme keys are documented under the .SH THEME KEYS section of the manual, one",
            "`.TP` + `.B <key>` per key, and in sdd/SCHEMA.md's per-key tables.",
            "Config settings go under .SH CONFIGURATION FILE, grouped by their `.SS [section]`.",
        ],
    )
}

/// Check 17 — the section 1 manual carries the attribution the About dialog shows.
///
/// This is not symmetry for its own sake. The syntect grammars are STATICALLY LINKED into
/// the binary and their licences require the notice to travel with it; the dialog tells
/// every user so, and POLICY records that the in-app claim is part of the obligation rather
/// than a description of it. A user on a headless box has no dialog — the manual is the
/// whole of what they get. So the attribution is checked, in one direction: every string
/// the dialog states must appear in the manual.
///
/// **ONE DIRECTION, DELIBERATELY.** The manual may say more (it has an OPTIONS section, a
/// FILES section and a licence line the dialog has no room for). Requiring the reverse
/// would make every sentence added to the manual a change to the dialog.
///
/// **AND IT CHECKS THE ATTRIBUTION, NOT THE DESCRIPTION.** The dialog's `.comments()` blurb
/// is prose the manual legitimately rewords for its own register, so pinning it would
/// force one wording on two audiences. That parity stays a review obligation; what is
/// gated here is the part that is a licence term, where the exact words are the point.
pub fn about_dialog_parity(tree: &Tree) -> bool {
    header("17", "About-dialog attribution present in the manual");
    let (Some(about), Some(man)) = (tree.text(ABOUT), tree.text(MAN1)) else {
        return fail(
            "an input this check reads is missing from the scan set",
            &[format!("expected both: {ABOUT}, {MAN1}")],
            &["Refusing to compare texts that were not read."],
        );
    };

    let dialog = vocab::about_text(about);
    if !dialog.is_well_formed() {
        // NOT an emptiness test, and the difference was measured: an emptiness test on a
        // flattened list passed with the website and BOTH credit sections extracting
        // nothing, because one surviving `authors` literal kept the list non-empty. Each
        // part is asserted separately so a partial break is a refusal rather than a
        // thinner comparison reported as a pass.
        return fail(
            &format!("{ABOUT}'s About dialog no longer has the shape this check reads"),
            &[format!(
                "extracted {} link(s), {} copyright line(s), {} credit section(s)",
                dialog.links.len(),
                usize::from(dialog.copyright.is_some()),
                dialog.credits.len(),
            )],
            &[
                "Expected a .website()/.website_label(), an .authors(vec![…]) and at least",
                "one add_credit_section() carrying a title and lines. Whichever changed,",
                "the extractor in xtask/src/lint/vocab.rs must be updated to match it —",
                "until then this check would compare a subset and report a pass.",
            ],
        );
    }
    let anchors = dialog.anchors();

    let rendered = vocab::roff_plain(man);
    let missing: Vec<String> = anchors
        .into_iter()
        .filter(|anchor| !rendered.contains(&vocab::roff_plain(anchor)))
        .map(|anchor| format!("{anchor:?}"))
        .collect();

    if missing.is_empty() {
        return pass();
    }
    fail(
        &format!("the About dialog states text {MAN1} does not carry"),
        &missing,
        &[
            "These come from add_about_action in src/app/appactions.rs. The credit sections",
            "are a licence obligation, not a courtesy: a reader with no display gets the",
            "manual instead of the dialog.",
            "Add them under .SH CREDITS / .SH COPYRIGHT / .SH SEE ALSO in the manual.",
            "roff escapes are decoded before the comparison, so write \\(em and \\(co as usual.",
        ],
    )
}
