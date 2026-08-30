//! Unit tests for the block-scope segmenter.
//!
//! The tokenizer's own rules (what a tight construct *is*) are tested in
//! [`super::super::scan`]; these are about what the BLOCK adds — a fence that
//! spans nested markup, and the four things that must not let one form.

use super::{segments_of, BlockScripts, Seg};
use crate::renderer::scan::Script;

/// Every rendered (non-delimiter) run of `md`, in document order, as the
/// renderer would emit them: the whole document flattened through the table.
fn rendered_runs(md: &str) -> Vec<(String, Script)> {
    let table = BlockScripts::scan(md);
    let mut out = Vec::new();
    for (ev, src) in
        pulldown_cmark::Parser::new_ext(md, crate::renderer::md_options()).into_offset_iter()
    {
        if let pulldown_cmark::Event::Text(t) = &ev {
            for seg in table.segments(src.start, t) {
                if !seg.marker {
                    out.push((seg.text(t).to_string(), seg.script));
                }
            }
        }
    }
    out
}

/// The source text of every construct the table accepted.
fn outer_sources(md: &str) -> Vec<String> {
    let table = BlockScripts::scan(md);
    table
        .outers()
        .iter()
        .map(|r| md[r.clone()].to_string())
        .collect()
}

// ── the defect this module exists for ─────────────────────────────────────────

#[test]
fn strike_fence_spans_nested_markup() {
    // pulldown splits this into `"~~a "`, Strong("bold"), `" b~~"`; the fence
    // must still be recognised and the `~~` must not render.
    assert_eq!(
        rendered_runs("~~a **bold** b~~\n"),
        vec![
            ("a ".into(), Script::Strikethrough),
            ("bold".into(), Script::Strikethrough),
            (" b".into(), Script::Strikethrough),
        ],
    );
    assert_eq!(
        outer_sources("~~a **bold** b~~\n"),
        vec!["~~a **bold** b~~"]
    );
}

#[test]
fn highlight_fence_spans_nested_markup() {
    assert_eq!(
        rendered_runs("==a *em* b==\n"),
        vec![
            ("a ".into(), Script::Highlight),
            ("em".into(), Script::Highlight),
            (" b".into(), Script::Highlight),
        ],
    );
}

#[test]
fn fence_spans_a_link() {
    assert_eq!(
        rendered_runs("~~see [docs](http://x/~~y) now~~\n"),
        vec![
            ("see ".into(), Script::Strikethrough),
            ("docs".into(), Script::Strikethrough),
            (" now".into(), Script::Strikethrough),
        ],
    );
    // The `~~` inside the DESTINATION is not a delimiter: a destination is no
    // `Text` event, so it never reaches the stitch.
    assert_eq!(
        outer_sources("~~see [docs](http://x/~~y) now~~\n"),
        vec!["~~see [docs](http://x/~~y) now~~"],
    );
}

#[test]
fn plain_fence_is_unchanged_by_the_block_pass() {
    assert_eq!(
        rendered_runs("~~struck~~ and H~2~O and E=mc^2^\n"),
        vec![
            ("struck".into(), Script::Strikethrough),
            (" and H".into(), Script::None),
            ("2".into(), Script::Subscript),
            ("O and E=mc".into(), Script::None),
            ("2".into(), Script::Superscript),
        ],
    );
}

// ── what must NOT form a fence ────────────────────────────────────────────────

#[test]
fn an_interleaved_fence_is_refused() {
    // The fence closes inside a Strong that opened inside it — not a tree. The
    // markers stay literal, exactly as before this module existed.
    assert_eq!(
        rendered_runs("~~a **b~~ c**\n"),
        vec![
            ("~~a ".into(), Script::None),
            ("b~~ c".into(), Script::None)
        ],
    );
    assert!(outer_sources("~~a **b~~ c**\n").is_empty());
}

#[test]
fn a_code_span_contributes_no_delimiter() {
    // The `~~` inside the code span is code, not a fence half.
    assert_eq!(
        rendered_runs("a `x ~~ y` b~~\n"),
        vec![("a ".into(), Script::None), (" b~~".into(), Script::None)],
    );
    assert!(outer_sources("a `x ~~ y` b~~\n").is_empty());
}

#[test]
fn an_images_alt_text_contributes_no_delimiter() {
    assert!(outer_sources("~~a ![alt ~~ here](i.png) b\n").is_empty());
}

#[test]
fn a_fence_does_not_span_a_block_boundary() {
    // Two paragraphs: the opener and the closer are in different blocks.
    assert!(outer_sources("~~open here\n\nclose here~~\n").is_empty());
    // Two table cells likewise.
    assert!(outer_sources("| ~~a | b~~ |\n|---|---|\n| c | d |\n").is_empty());
}

#[test]
fn a_line_break_still_closes_a_tight_script() {
    // A soft break is whitespace, so `~x~` cannot span it — but a `~~` fence,
    // whose content may contain spaces, can.
    assert!(outer_sources("a~b\nc~d\n").is_empty());
    assert_eq!(outer_sources("~~a\nb~~\n"), vec!["~~a\nb~~"]);
}

#[test]
fn a_fence_inside_markup_is_accepted() {
    // Proper nesting the other way round: the construct sits inside the Strong.
    assert_eq!(
        rendered_runs("**bold ~~struck~~**\n"),
        vec![
            ("bold ".into(), Script::None),
            ("struck".into(), Script::Strikethrough),
        ],
    );
}

// ── segment partitioning ──────────────────────────────────────────────────────

#[test]
fn segments_partition_the_event_completely() {
    // Concatenating EVERY segment (markers included) must rebuild the run —
    // this is the invariant `copymap` reconstructs an event's source from.
    for text in ["plain", "~~x~~", "E=mc^2^", "a ==b c== d", "H~2~O"] {
        let rebuilt: String = segments_of(text)
            .iter()
            .map(|s| s.text(text))
            .collect::<Vec<_>>()
            .concat();
        assert_eq!(rebuilt, text, "segments must partition {text:?}");
    }
}

#[test]
fn segments_carry_the_delimiters_on_the_right_events() {
    let md = "~~a **bold** b~~\n";
    let table = BlockScripts::scan(md);
    // Event 1 opens the fence; event 3 closes it; the nested run carries neither.
    let opener: Vec<Seg> = table.segments(0, "~~a ");
    assert_eq!(opener.first().map(|s| s.marker), Some(true));
    assert_eq!(opener.first().map(|s| s.text("~~a ")), Some("~~"));
    assert_eq!(opener.last().map(|s| s.script), Some(Script::Strikethrough));

    let nested: Vec<Seg> = table.segments(6, "bold");
    assert_eq!(nested.len(), 1);
    assert!(!nested[0].marker);
    assert_eq!(nested[0].script, Script::Strikethrough);

    let closer: Vec<Seg> = table.segments(12, " b~~");
    assert_eq!(closer.last().map(|s| s.marker), Some(true));
    assert_eq!(closer.last().map(|s| s.text(" b~~")), Some("~~"));
}

#[test]
fn an_unknown_event_falls_back_to_a_standalone_scan() {
    // The documented degradation: a src offset from a different parse still
    // segments correctly for constructs wholly inside the run.
    let table = BlockScripts::scan("unrelated\n");
    assert_eq!(
        table.segments(9_999, "~~x~~"),
        segments_of("~~x~~"),
        "an unknown event must degrade to the single-run scan",
    );
}
