//! **The two find engines, measured against each other.**
//!
//! The find bar drives two entirely separate matchers: `GtkSourceSearchContext` over the
//! editor's source buffer, and [`super::matcher::Matcher`] over the preview's three
//! texts. TDD 11.13 and 11.14 claim a query means the *same thing* in both panes. That
//! claim is not provable by reading either implementation — it is a claim about an
//! agreement between them, so the only evidence that settles it is running both over one
//! fixture and comparing the counts.
//!
//! Everything here is therefore a comparison, never an assertion about one engine's
//! output in isolation: a case that hard-codes the number it expects stops being able to
//! tell you the engines have drifted, which is the entire failure this file exists to
//! catch.
//!
//! **This is a probe that became a gate.** Five questions about GtkSourceView's own
//! regular-expression handling decided the matcher's design — how whole-word wraps a
//! pattern, which characters bound a word, which compile flags are set, and what a
//! zero-width pattern does. Each was answered here by measurement rather than by reading
//! GtkSourceView's C source, because a source reading proves what one engine does and
//! this file has to prove what two engines do *together*. The answers are recorded beside
//! the case that measured them.
//!
//! The fixtures are deliberately plain text with no Markdown structure: the subject is
//! the matcher, and a fixture the renderer would transform would measure the renderer
//! too.

use super::matcher::Matcher;
use super::options::FindOptions;
use gtk::prelude::*;
use sourceview::prelude::*;

/// Count `query` in `hay` with the editor's engine, under `opts`.
///
/// Pumps until `occurrences-count` stops answering the `-1` "still scanning" sentinel,
/// because a count read before the scan settles is not a count (see
/// `super::decode_occurrence_total`). A malformed pattern never settles — the engine
/// reports the error instead — so that case returns `Err` rather than spinning.
fn editor_count(hay: &str, query: &str, opts: FindOptions) -> Result<i32, String> {
    let buf = sourceview::Buffer::new(None);
    buf.set_text(hay);
    let settings = sourceview::SearchSettings::new();
    // Exactly what the find bar hands the engine — the application's own wrapping for a
    // whole-word regular expression, and the engine's own for a literal
    // (`matcher::configure_engine_options` / `configure_engine_query`, the functions the
    // find bar itself calls). Setting the RAW
    // query here instead would measure an engine the application never drives, which is
    // how this file came to record a false conclusion once already.
    super::matcher::configure_engine_options(&settings, opts);
    super::matcher::configure_engine_query(&settings, query, opts);
    let sc = sourceview::SearchContext::new(&buf, Some(&settings));
    crate::testpump::until(
        crate::testpump::Clock::Idle,
        "the editor search context to finish scanning the fixture",
        || sc.occurrences_count() >= 0 || sc.regex_error().is_some(),
    );
    match sc.regex_error() {
        Some(e) => Err(e.message().to_string()),
        None => Ok(sc.occurrences_count()),
    }
}

/// Count `query` in `hay` with the preview's matcher, under the same options.
fn preview_count(hay: &str, query: &str, opts: FindOptions) -> Result<i32, String> {
    match Matcher::compile(query, opts) {
        Ok(m) => Ok(m.count(hay) as i32),
        Err(e) => Err(e.message),
    }
}

/// Assert both engines answer the same thing for `query` over `hay`.
///
/// `why` names the property being measured, so a failure says which agreement broke
/// rather than only which numbers differ.
fn agree(hay: &str, query: &str, opts: FindOptions, why: &str) {
    let editor = editor_count(hay, query, opts);
    let preview = preview_count(hay, query, opts);
    match (&editor, &preview) {
        (Ok(a), Ok(b)) => assert_eq!(
            a, b,
            "{why}: the editor found {a} and the preview found {b} for {query:?} \
             under {opts:?} — the same query must mean the same thing in both panes"
        ),
        (Err(_), Err(_)) => (),
        _ => panic!(
            "{why}: one engine accepted {query:?} under {opts:?} and the other rejected it \
             (editor: {editor:?}, preview: {preview:?}) — a pattern accepted in one pane \
             must be accepted in the other"
        ),
    }
}

fn opts(case_sensitive: bool, whole_word: bool, regex: bool) -> FindOptions {
    FindOptions {
        case_sensitive,
        whole_word,
        regex,
    }
}

/// **The baseline.** A plain literal under each of the four case/word combinations must
/// count the same in both panes, or nothing measured below means anything.
/// TDD 11.13.
#[gtktest::test]
fn a_literal_query_counts_the_same_in_both_panes() {
    const HAY: &str = "note Note NOTE notebook footnote note_2 note2 note-2\n";
    for case_sensitive in [false, true] {
        for whole_word in [false, true] {
            agree(
                HAY,
                "note",
                opts(case_sensitive, whole_word, false),
                "literal baseline",
            );
        }
    }
}

/// **Question 2 — which characters bound a word.** The matcher's literal whole-word
/// predicate is `is_alphanumeric() || '_'`; GtkSourceView's is the `gtk_text_iter` word
/// family. These four candidates are exactly where two such predicates part company:
/// `note_2` (underscore), `note2` (digit), `note-2` (hyphen) and a non-ASCII letter.
///
/// MEASURED: they agree. The matcher keeps its own predicate.
/// TDD 11.13.
#[gtktest::test]
fn whole_word_draws_the_word_boundary_at_the_same_characters() {
    for hay in [
        "note_2 and more\n",
        "note2 and more\n",
        "note-2 and more\n",
        "notés and more\n",
        "über note über\n",
        "a note. note, note; note!\n",
    ] {
        agree(hay, "note", opts(false, true, false), "word boundary");
    }
}

// Whole word over a regular-expression ALTERNATION is not a parity case here: both
// panes compile the same `editor_pattern` string, so a case built from it agrees
// whatever the wrapper is. GtkSourceView's own wrapper is ungrouped (`matcher.rs`), and
// the guard that fails when the application's wrapping regresses drives the find bar:
// `bartests::whole_word_bounds_an_alternation_the_same_way_in_both_panes`.

/// **Question 3 — compile flags.** The matcher compiles with MULTILINE, so `^` anchors
/// at every line start rather than only at the start of the text. If GtkSourceView did
/// not, `^` would mean two different things in the two panes — the most visible possible
/// divergence, since it changes the count on any multi-line document.
///
/// MEASURED: both anchor per line. `$` is measured with it, since the same flag governs
/// both.
/// TDD 11.14.
#[gtktest::test]
fn an_anchor_means_the_same_thing_on_a_multi_line_document() {
    const HAY: &str = "alpha one\nbeta two\nalpha three\ngamma alpha\n";
    for query in ["^alpha", "alpha$", "^[ab][a-z]+", r"\w+$"] {
        agree(
            HAY,
            query,
            opts(false, false, true),
            "anchor under MULTILINE",
        );
    }
}

/// A character class, an anchor and a quantifier together — the shape rubric 11.14
/// names, and the one a reader actually types.
#[gtktest::test]
fn a_compound_pattern_counts_the_same_in_both_panes() {
    const HAY: &str = "item 1\nitem 22\nitem 333\nnot an item\nITEM 4\n";
    for query in [r"^item \d+", r"[0-9]{2,}", r"it.m", r"(?:item|thing)\s*\d*"] {
        for case_sensitive in [false, true] {
            agree(
                HAY,
                query,
                opts(case_sensitive, false, true),
                "compound pattern",
            );
        }
    }
}

/// **Question 4 — a pattern that can match nothing.** `a*` matches at every position,
/// so a naive "resume at the match end" loop never advances. The matcher has a progress
/// guard that truncates rather than hangs; the question was whether truncating puts it
/// out of step with the editor.
///
/// MEASURED: both engines report a match at every position, and they report the same
/// number of them — the guard never fires on a fixture this size, so it is a hang
/// stopper rather than a behaviour difference. Recorded here because "the preview is
/// wrong to truncate" was the live hypothesis, and it is not.
/// TDD 11.14.
#[gtktest::test]
fn a_zero_width_pattern_does_not_diverge() {
    for hay in ["aaa\n", "abcabc\n", "\n"] {
        for query in ["a*", r"\b", "x?"] {
            agree(hay, query, opts(false, false, true), "zero-width pattern");
        }
    }
}

/// A malformed pattern is an ERROR in both panes, never a zero count. This is the
/// agreement rubric 11.15 rests on: the readout can only say "invalid pattern" for both
/// panes if both panes can say it.
#[gtktest::test]
fn a_malformed_pattern_is_rejected_by_both_engines() {
    for query in ["(unclosed", "a{2,1}", r"[z-a]", "*leading"] {
        let editor = editor_count("some text here\n", query, opts(false, false, true));
        let preview = preview_count("some text here\n", query, opts(false, false, true));
        assert!(
            editor.is_err(),
            "the editor accepted the malformed pattern {query:?}: {editor:?}"
        );
        assert!(
            preview.is_err(),
            "the preview accepted the malformed pattern {query:?}: {preview:?}"
        );
    }
}

/// An empty query matches nothing, in every mode. In regular-expression mode an empty
/// pattern would otherwise match at every position and report a count equal to the
/// document's length — "no query" is not "a query that matches everywhere".
#[gtktest::test]
fn an_empty_query_matches_nothing_in_either_pane() {
    for regex in [false, true] {
        let o = opts(false, false, regex);
        assert_eq!(preview_count("aaa bbb\n", "", o), Ok(0));
        assert_eq!(
            editor_count("aaa bbb\n", "", o),
            Ok(0),
            "the editor's engine treats an unset search text as no matches"
        );
    }
}

/// **Question 5 — what a replacement string means under a regular expression.**
///
/// Replace is the editor's alone (the preview is read-only), so this is not a
/// comparison between engines: it is a probe of the one engine that performs it, kept
/// here because it belongs with the other four answers and because the claim it settles
/// is in the same rubric family (TDD 11.17).
///
/// MEASURED, and one half came out the opposite way to the guess:
///
/// - A backreference **does** expand against the match it replaced, so `\2 \1` over
///   `(\w+) (\w+)` swaps the two words. That is the behaviour rubric 11.17 asserts.
/// - A reference to a group the pattern does **not** have is **accepted and expands to
///   nothing** — `\9` deletes the match rather than being rejected or inserted
///   literally. It is GRegex's rule, not GtkSourceView's, and it is why the replace
///   path cannot treat "no error" as "the reader got what they asked for": there is no
///   diagnostic to surface, because the engine does not consider it a mistake.
/// - A replacement that is genuinely malformed — a trailing lone backslash — IS
///   rejected, which is the case the error path exists for.
#[gtktest::test]
fn a_replacement_expands_a_backreference_against_its_match() {
    /// Replace the fixture's single match with `replacement`, and answer what the
    /// document became. Re-derives the match each time: a `GtkTextIter` does not
    /// survive the `set_text` that resets the fixture.
    fn replace_once(replacement: &str) -> Result<String, String> {
        let buf = sourceview::Buffer::new(None);
        buf.set_text("alpha beta\n");
        let settings = sourceview::SearchSettings::new();
        settings.set_regex_enabled(true);
        settings.set_search_text(Some(r"(\w+) (\w+)"));
        let sc = sourceview::SearchContext::new(&buf, Some(&settings));
        crate::testpump::until(
            crate::testpump::Clock::Idle,
            "the search context to find the fixture's one match",
            || sc.occurrences_count() >= 1,
        );
        let (mut ms, mut me) = sc
            .forward(&buf.start_iter())
            .map(|(ms, me, _)| (ms, me))
            .expect("the pattern matches the fixture");
        match sc.replace(&mut ms, &mut me, replacement) {
            Ok(()) => Ok(crate::saferizer::BufferText::of(&buf).into_string()),
            Err(e) => Err(e.message().to_string()),
        }
    }

    assert_eq!(
        replace_once(r"\2 \1").as_deref(),
        Ok("beta alpha\n"),
        "a backreference must expand against the match it replaced, not appear literally"
    );
    assert_eq!(
        replace_once(r"\9").as_deref(),
        Ok("\n"),
        "a reference to a group the pattern does not have expands to NOTHING and is not \
         an error — so a silent deletion is a thing the reader can ask for by accident, \
         and there is no diagnostic for the replace path to surface"
    );
    assert!(
        replace_once("\\").is_err(),
        "a trailing lone backslash is the malformed replacement the error path exists for"
    );
}
