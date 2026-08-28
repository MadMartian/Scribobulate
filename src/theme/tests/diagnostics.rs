//! What a refused key SAYS (TDD 18.33/18.35, SCHEMA § Key resolution).
//!
//! Every refusal in this engine is inert by design — the key falls through and the
//! surface inherits — so the only observable that separates "the theme said nothing"
//! from "the theme said something this build would not take" is the log record. Nine
//! `warn` sites were in that position with nothing watching any of them: a refactor
//! that downgraded one to `debug!`, dropped the theme id, or deleted it outright would
//! have passed every gate while producing exactly the silence SCHEMA forbids.

use super::super::spec::ThemeSpec;
use super::super::*;
use crate::testlog;
use log::Level::Warn;

fn parse_one(id: &str, body: &str) -> ThemeSpec {
    let specs = Themes::parse_compiled(&format!("[themes.{id}]\n{body}"))
        .expect("the fixture parses as a themes file");
    specs
        .get(id)
        .cloned()
        .expect("the fixture defines the theme")
}

/// TDD 18.33 — an unrecognised key is ignored **and reported at `warn`, naming the
/// theme id and the key**.
///
/// Both halves matter and only one was pinned: `spec.rs`'s existing tests observe that
/// the theme SURVIVES an unknown key, which a build that logged nothing at all would
/// also satisfy.
#[test]
fn an_unknown_key_is_reported_at_warn_naming_the_theme_and_the_key() {
    let cap = testlog::capture();
    let spec = parse_one(
        "acme",
        "heading_colour = \"#00ff00\"\nlink_color = \"#0000ff\"\n",
    );
    assert_eq!(spec.spellings(), vec!["link_color"], "the key is dropped");
    assert!(
        cap.logged(Warn, "heading_colour"),
        "the refusal must name the key: {:?}",
        cap.records()
    );
    assert!(
        cap.logged(Warn, "acme"),
        "the refusal must name the theme: {:?}",
        cap.records()
    );
    // …and the key beside it, which applied fine, earns no complaint of its own.
    assert!(!cap.logged(Warn, "link_color"));
}

/// TDD 18.35 — a **wrong-typed** value is reported the same way, and that includes the
/// retired ARRAY spellings.
///
/// `heading_scale = [2.2, 1.8]` is the spelling this vocabulary replaced. It is not an
/// unknown key — the registry claims `heading_scale` — so it takes the wrong-type path
/// rather than the unknown-key one, which is why the existing retired-spelling coverage
/// in `keys.rs` did not reach it.
#[test]
fn a_wrong_typed_value_is_reported_including_the_retired_array_spelling() {
    let cap = testlog::capture();
    let spec = parse_one("acme", "list_step = \"wide\"\nlist_item_gap = 12\n");
    assert_eq!(spec.spellings(), vec!["list_item_gap"]);
    assert!(cap.logged(Warn, "list_step"), "{:?}", cap.records());
    assert!(cap.logged(Warn, "a whole number"), "{:?}", cap.records());
    drop(cap);

    let cap = testlog::capture();
    let spec = parse_one("acme", "heading_scale = [2.2, 1.8]\n");
    assert!(
        spec.spellings().is_empty(),
        "an array-valued heading_scale is a retired spelling, not a value"
    );
    assert!(cap.logged(Warn, "heading_scale"), "{:?}", cap.records());
    assert!(cap.logged(Warn, "acme"), "{:?}", cap.records());
}

/// **A value the typed parse refuses names the theme and the key too.**
///
/// This is the third refusal class, and it was the silent one. `parse_color` had no
/// log at all — and colours are roughly half the vocabulary, so the most typo-prone
/// value class had no diagnostic whatever. The other three value parsers did log, but
/// anonymously: "theme: unknown line style" left the reader to find which of seven
/// themes, and which of ~150 spellings, had said it.
#[test]
fn a_value_the_parser_refuses_is_reported_with_its_theme_and_its_key() {
    let cap = testlog::capture();
    let themes = {
        let mut t = Themes::builtin();
        t.merge_over_for_test("[themes.acme]\nlink_color = \"chartreusey\"\n");
        t
    };
    let resolved = themes.resolve("acme");
    assert!(
        resolved.link_color.is_none()
            || resolved.link_color == Themes::builtin().resolve(SYSTEM_ID).link_color,
        "a refused colour must fall through, never land"
    );
    assert!(cap.logged(Warn, "link_color"), "{:?}", cap.records());
    assert!(cap.logged(Warn, "acme"), "{:?}", cap.records());
    assert!(cap.logged(Warn, "chartreusey"), "{:?}", cap.records());
}

/// A refusal is reported **once**, not once per level.
///
/// A heading key's fallback chain revisits its bare spelling at every one of the five
/// levels, and a depth key's at every tier — so one bad value used to be parsed, and
/// complained about, five times. The walk now records the refusal and skips the
/// re-parse, which is what makes the count right; the answer is unchanged because the
/// parse is pure over one value.
#[test]
fn one_bad_value_earns_one_diagnostic_not_one_per_level() {
    let cap = testlog::capture();
    let mut themes = Themes::builtin();
    themes.merge_over_for_test("[themes.acme]\nheading_color = \"not a colour\"\n");
    let _ = themes.resolve("acme");
    let complaints = cap
        .records()
        .into_iter()
        .filter(|r| r.message.contains("not a colour"))
        .count();
    assert_eq!(
        complaints,
        1,
        "one bad heading_color earned {complaints} records: {:?}",
        cap.records()
    );
}

/// **The shipped themes file states zero keys this build refuses.**
///
/// `data/themes.toml` is the file a human reads to learn the vocabulary, so a retired
/// spelling surviving in it is documentation that teaches the wrong thing — and with
/// nothing watching the log, it would ship undetected. Driven through the same
/// `Themes::parse` the application uses, so it sees exactly the refusals a user's copy
/// would.
#[test]
fn the_shipped_themes_file_states_no_key_this_build_refuses() {
    let cap = testlog::capture();
    let parsed = Themes::parse_compiled(BUILTIN_THEMES_TOML);
    assert!(parsed.is_some(), "the shipped themes file must parse");
    let complaints: Vec<String> = cap
        .records()
        .into_iter()
        .filter(|r| r.level == Warn)
        .map(|r| r.message)
        .collect();
    assert!(
        complaints.is_empty(),
        "data/themes.toml states keys this build refuses: {complaints:#?}"
    );
}

/// Anti-vacuity for the guard above: it can see a bad key in a file of that shape.
///
/// Without this, "no complaints" is satisfiable by a capture that was never armed or a
/// parse that never ran — the exact false pass a silence assertion invites.
#[test]
fn the_shipped_file_guard_can_see_a_bad_key() {
    let cap = testlog::capture();
    let _ = Themes::parse_compiled(&format!(
        "{BUILTIN_THEMES_TOML}\n[themes.probe]\nheading_colour = \"#ff0000\"\n"
    ));
    assert!(
        cap.logged(Warn, "heading_colour"),
        "the sweep above proves nothing if a bad key is invisible to it"
    );
}

/// TDD 18.46 — **a key that is shadowed at every level it could apply to is reported,
/// naming the theme, the key, and what shadows it.**
///
/// This is the measured defect. `[themes.system]` states `heading_space_above_h1`…`_h5`
/// (`data/themes.toml`), so a user file stating the bare `heading_space_above` over it
/// loses at every one of the five levels — and the key is recognised, is the right TOML
/// type, and parses, so none of the other two refusal paths above sees it. Before this
/// diagnostic the file parsed clean, changed nothing, and said nothing.
///
/// **It was LOUDER at this branch's point.** `heading_space_below` was array-valued
/// then, so a bare scalar was a TOML type error that rejected the whole user file with
/// a `warn`. The value semantics got better and the diagnostic got worse; this is what
/// puts it back.
#[test]
fn a_key_shadowed_at_every_level_is_reported_with_the_theme_the_key_and_the_shadow() {
    let mut themes = Themes::builtin();
    themes.merge_over_for_test("[themes.system]\nheading_space_above = 20\n");

    // First the half that makes the diagnostic necessary: the key applies NOWHERE.
    let above = themes.resolve(SYSTEM_ID).metrics.heading_space_above;
    assert_eq!(
        above, [0; HEADING_LEVELS],
        "the shipped narrowed keys must still win at every level — the resolution \
         order is correct and is not what this test is about"
    );

    let cap = testlog::capture();
    themes.warn_on_shadowed_keys();
    assert!(
        cap.logged(Warn, "heading_space_above"),
        "the report must name the key: {:?}",
        cap.records()
    );
    assert!(
        cap.logged(Warn, "system"),
        "the report must name the theme: {:?}",
        cap.records()
    );
    assert!(
        cap.logged(Warn, "heading_space_above_h5"),
        "the report must name what shadows it, or the reader cannot act on it: {:?}",
        cap.records()
    );
}

/// **A narrowed key stated beside the shadowed one is NOT itself reported.**
///
/// `heading_space_below_h4 = 44` is the spelling that works, in the same user file as
/// the bare key that does not. A diagnostic that complained about it too would be
/// telling the author to stop doing the one thing they got right.
#[test]
fn the_narrowed_key_that_does_apply_earns_no_report() {
    let mut themes = Themes::builtin();
    themes.merge_over_for_test(
        "[themes.system]\nheading_space_above = 20\nheading_space_below_h4 = 44\n",
    );
    assert_eq!(
        themes.resolve(SYSTEM_ID).metrics.heading_space_below[3],
        44,
        "the narrowed key applies, which is why it must not be reported"
    );

    let cap = testlog::capture();
    themes.warn_on_shadowed_keys();
    let about_below: Vec<String> = cap
        .records()
        .into_iter()
        .filter(|r| r.message.contains("heading_space_below"))
        .map(|r| r.message)
        .collect();
    assert!(
        about_below.is_empty(),
        "a key that applies must earn no report: {about_below:#?}"
    );
}

/// **A bare key shadowed at SOME levels and winning at others is not reported.**
///
/// This is the precision half, and it is the one that decides whether anybody reads
/// these records at all. A theme stating `heading_space_above` plus
/// `heading_space_above_h1` has narrowed h1 and left h2–h5 to the bare key — which is
/// the vocabulary working as designed (TDD 18.32), not a mistake.
#[test]
fn a_bare_key_that_still_wins_at_one_level_is_not_reported() {
    let mut themes = Themes::builtin();
    themes.merge_over_for_test(
        "[themes.acme]\nheading_space_above = 20\nheading_space_above_h1 = 3\n",
    );
    let above = themes.resolve("acme").metrics.heading_space_above;
    assert_eq!(
        above,
        [3, 20, 20, 20, 20],
        "the bare key must genuinely still apply at h2–h5"
    );

    let cap = testlog::capture();
    themes.warn_on_shadowed_keys();
    let complaints: Vec<String> = cap
        .records()
        .into_iter()
        .filter(|r| r.message.contains("heading_space_above"))
        .map(|r| r.message)
        .collect();
    assert!(
        complaints.is_empty(),
        "a bare key that applies at four levels is doing its job: {complaints:#?}"
    );
}

/// **A key with a consumer that reads it BARE is not reported, even fully narrowed.**
///
/// `heading_color` is read bare by the table header when the theme states no
/// `table_head_fg` (TDD 18.30), so a theme that narrows all five heading levels has
/// NOT made it unreachable. The registry declares that reader (`keys::Key::bare_reader`);
/// this is the test that the declaration is consulted rather than decorative.
#[test]
fn a_bare_key_with_its_own_consumer_is_not_reported_however_narrowed() {
    let narrowed: String = (1..=HEADING_LEVELS)
        .map(|n| format!("heading_color_h{n} = \"#00000{n}\"\n"))
        .collect();
    let mut themes = Themes::builtin();
    themes.merge_over_for_test(&format!(
        "[themes.acme]\nheading_color = \"#ff0000\"\n{narrowed}"
    ));

    let resolved = themes.resolve("acme");
    assert_eq!(
        resolved.heading_color,
        parse_color("#ff0000"),
        "the bare key still reaches the table header, which is why it is not shadowed"
    );
    assert_eq!(
        resolved.heading_colors[0],
        parse_color("#000001"),
        "…while every heading level takes its own narrowed value"
    );

    let cap = testlog::capture();
    themes.warn_on_shadowed_keys();
    let complaints: Vec<String> = cap
        .records()
        .into_iter()
        .filter(|r| r.message.contains("heading_color"))
        .map(|r| r.message)
        .collect();
    assert!(
        complaints.is_empty(),
        "heading_color still applies to the table header: {complaints:#?}"
    );
}

/// **The shipped themes file states zero keys that can never apply.**
///
/// The same discipline as `the_shipped_themes_file_states_no_key_this_build_refuses`
/// above, applied to the third refusal class: `data/themes.toml` is the file a human
/// reads to learn the vocabulary, so a key in it that does nothing is documentation
/// teaching a dead spelling. If this ever fires, the predicate is wrong or the data is
/// — read the record before touching either.
#[test]
fn the_shipped_themes_state_no_key_that_can_never_apply() {
    let themes = Themes::builtin();
    let cap = testlog::capture();
    themes.warn_on_shadowed_keys();
    let complaints: Vec<String> = cap
        .records()
        .into_iter()
        .filter(|r| r.level == Warn)
        .map(|r| r.message)
        .collect();
    assert!(
        complaints.is_empty(),
        "data/themes.toml states keys that can never apply: {complaints:#?}"
    );
}

/// Anti-vacuity for the sweep above: it can see a shadowed key in a file of that shape.
///
/// Without it, "no complaints" is satisfiable by a sweep that never ran or a capture
/// that was never armed — the false pass every silence assertion invites.
#[test]
fn the_shipped_shadow_guard_can_see_a_shadowed_key() {
    let narrowed: String = (1..=HEADING_LEVELS)
        .map(|n| format!("heading_space_above_h{n} = {n}\n"))
        .collect();
    let mut themes = Themes::builtin();
    themes.merge_over_for_test(&format!(
        "[themes.probe]\nheading_space_above = 20\n{narrowed}"
    ));
    let cap = testlog::capture();
    themes.warn_on_shadowed_keys();
    assert!(
        cap.logged(Warn, "heading_space_above"),
        "the sweep above proves nothing if a shadowed key is invisible to it"
    );
}

/// **The sweep is on the load path, not only on a method the tests call.**
///
/// The measured defect arrives through a themes file on disk, so the guard has to
/// follow it there. `assemble` is `load` minus the `std::env` read — the same seam
/// `SearchBases` is — so this drives the real built-in parse, the real merge and the
/// real diagnostics with a real file's text, and would go silent if the sweep were ever
/// unwired from that path (ScrAP-324's second half: a forgotten caller is invisible to
/// a guard that calls the function itself).
#[test]
fn a_themes_file_on_the_load_path_earns_the_report() {
    let dir = tempfile::tempdir().expect("a scratch directory for the themes file");
    let body = "[themes.system]\nheading_space_above = 20\n".to_string();

    let cap = testlog::capture();
    let themes = super::super::assemble(Some((body, dir.path().to_path_buf())));
    assert!(
        cap.logged(Warn, "heading_space_above"),
        "the load path must report it: {:?}",
        cap.records()
    );
    drop(cap);
    assert_eq!(
        themes.resolve(SYSTEM_ID).metrics.heading_space_above,
        [0; HEADING_LEVELS],
        "…and the resolution order is unchanged, which is the point of reporting \
         rather than repairing"
    );
}
