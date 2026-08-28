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
