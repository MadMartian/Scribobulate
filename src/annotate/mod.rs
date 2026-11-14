//! CriticMarkup annotation/review support.
//!
//! This module is the **pure**, display-free core of the annotation feature: it
//! knows how to write a CriticMarkup annotation *into* a Markdown source string
//! ([`mutate`]) and — later — how to scan annotations back *out* of it. Keeping
//! it pure (no widget access, no GTK types) mirrors the discipline of
//! [`crate::copymap`] and makes the risky offset/string surgery exhaustively
//! unit-testable with plain `cargo test`.
//!
//! # Storage format
//!
//! Annotations are stored inline in the Markdown file as CriticMarkup — the
//! de-facto plain-text standard for editorial markup — so they survive external
//! round-trips and stay human-legible in any editor:
//!
//! - **Highlight + comment**: `{==highlighted claim==}{>>reviewer comment<<}`
//! - **Point comment** (no highlighted span, used for the cross-block fallback):
//!   `{>>reviewer comment<<}`
//!
//! v1 renders only these two; the suggested-edit kinds (`{++ins++}`,
//! `{--del--}`, `{~~a~>b~~}`) are recognised but styled inert (deferred, not
//! designed out — see [`scan::AnnKind`]).

pub(crate) mod landing;
pub(crate) mod mutate;
pub(crate) mod scan;
mod written;

pub(crate) use landing::Landing;
#[cfg(test)]
pub(crate) use landing::Slot;
pub(crate) use mutate::{
    edit_comment, insert_landed_point, insert_or_extend_highlight, merged_comment_for,
    remove_annotation,
};
pub(crate) use scan::{
    chip_anchor, cleaned_to_original, extract, original_to_cleaned, AnnKind, Annotation,
};
pub(crate) use written::holds_block_break;

/// A claim's text as the page SHOWS it: the run of cleaned source the highlight covers
/// with its Markdown read, not quoted. `**bold claim**` is `bold claim`, a link is its
/// label, a code span its code, and the tight constructs this crate scans itself
/// (`==x==`, `~~x~~`, `^x^`, `~x~`) lose their delimiters, exactly as the preview drew
/// them (TDD 17.69).
///
/// The ONE reduction every surface that quotes a claim takes — the margin chip's card,
/// the annotations viewer's rows and the export's summary — so they cannot quote the same
/// claim three ways. Per-event rendered text comes from the block-scope [`BlockScripts`]
/// the renderer cuts runs with, so a fence whose two halves sit in different events
/// (`~~a **bold** b~~`) is stripped as the page strips it.
pub(crate) fn plain_claim(raw: &str) -> String {
    use pulldown_cmark::Event;
    // Read the way every document is: normalised first, so the scan and the parse index
    // one text.
    let md = crate::renderer::NormalizedMd::new(raw);
    let scripts = crate::renderer::BlockScripts::scan(md.as_str());
    let mut out = String::with_capacity(raw.len());
    for (event, range) in md.parse_with_ranges() {
        match event {
            Event::Text(text) => scripts.push_rendered_text(range.start, &text, &mut out),
            Event::Code(code) => out.push_str(&code),
            Event::SoftBreak | Event::HardBreak => out.push(' '),
            // Structure and markup the page shows as no words of the claim's own: a
            // tag's start and end (its text arrives as `Text`), a rule, a task box, raw
            // HTML and footnote or math markers (math is off in `md_options`).
            Event::Start(_)
            | Event::End(_)
            | Event::Rule
            | Event::TaskListMarker(_)
            | Event::Html(_)
            | Event::InlineHtml(_)
            | Event::FootnoteReference(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_) => {}
        }
    }
    out
}

/// Chars of `run[..upto]` that survive into the rendered buffer: everything except
/// the **delimiters** of the tight constructs the renderer strips (`==mark==`,
/// `~~strike~~`, `^sup^`, `~sub~`). `segs` is the run's segmentation from
/// [`crate::renderer::BlockScripts`] — the block-scope one the renderer cuts runs
/// with, so a fence whose two halves sit in different events is seen exactly as the
/// renderer sees it. Content between markers stays 1:1 and maps char-precisely; only
/// the markers vanish.
///
/// An `upto` landing inside a delimiter run clamps to the last kept char before it,
/// which is what a claim boundary sitting on a marker means — the marker is not in
/// the buffer, so there is no position "inside" it to point at.
fn kept_chars(run: &str, segs: &[crate::renderer::Seg], upto: usize) -> i32 {
    segs.iter()
        .filter(|seg| !seg.marker && seg.range.start < upto)
        .map(|seg| {
            run.get(seg.range.start..seg.range.end.min(upto))
                .map_or(0, |t| t.chars().count() as i32)
        })
        .sum()
}

/// Map a cleaned-source highlight span `[hs, he)` onto **cell-local** (or buffer-local)
/// char ranges using content-event records `(src_start, src_end, buf_before, buf_after)`.
///
/// **The single mapper for both annotation-display paths** — the table cells' Pango
/// markup and, via `preview/build.rs::highlight_tag_ranges`, the body buffer's
/// `annotation-highlight` tag. It used to be two character-identical copies, which
/// is how one could have been fixed while the other stayed wrong (ScrAP-194).
///
/// Per content event, the claim's overlap maps char-precisely whenever the chars the
/// buffer actually received can be accounted for; a run whose length the renderer
/// changed in a way we *cannot* account for is tagged whole, never split mid-glyph.
/// The accounting is `kept_chars`: a construct's delimiters are dropped, everything
/// else is 1:1. So the precise path covers both a plain 1:1 run (no constructs —
/// no marker segments, `kept == run chars`) and a **marker-stripped** run
/// (`a ==mark== b` → `a mark b`), which is mappable precisely *because* the marker
/// positions are known exactly.
///
/// The whole-event fallback then keeps exactly the case it was written for: a
/// genuinely synthesised run (smart punctuation `--`→`–`, `...`→`…`, an entity),
/// where there is no per-character correspondence to find. Equality is a sound test
/// for "fully accounted for" because every transform in this pipeline only ever
/// *removes* chars — so if a synthesised substitution also shrank the run, the
/// counts cannot match and we fall back rather than mis-map.
pub(crate) fn map_cleaned_highlight_to_local(
    cleaned: &str,
    hs: usize,
    he: usize,
    content_evs: &[(usize, usize, i32, i32)],
    scripts: &crate::renderer::BlockScripts,
) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for &(s, e, before, after) in content_evs {
        let ov_s = hs.max(s);
        let ov_e = he.min(e);
        if ov_s >= ov_e {
            continue;
        }
        if s > e || e > cleaned.len() || s > cleaned.len() {
            continue;
        }
        let run = &cleaned[s..e];
        let buf_len = (after - before) as usize;
        // The BLOCK-scope segmentation, keyed by the event's source start — never a
        // scan of `run` alone, which cannot see a fence whose partner sits in another
        // event (`~~a **bold** b~~`): it counted both halves' `~~` as kept text, failed
        // the length check below, and tagged each WHOLE event, washing the prose either
        // side of the claim (MANUAL-TEST 10.10b). Segments index the event's RENDERED
        // text, so they apply to `run` only when they span it exactly; the kept-count
        // check then confirms the 1:1 correspondence.
        let segs = scripts.segments(s, run);
        let spans_run = segs.last().is_some_and(|seg| seg.range.end == run.len());
        if spans_run && kept_chars(run, &segs, run.len()) as usize == buf_len {
            out.push((
                before + kept_chars(run, &segs, ov_s - s),
                before + kept_chars(run, &segs, ov_e - s),
            ));
        } else {
            out.push((before, after));
        }
    }
    out
}

#[cfg(test)]
mod highlight_map_tests {
    use super::map_cleaned_highlight_to_local;
    use crate::renderer::BlockScripts;

    /// The mapper over HAND-BUILT event records, which no parse produced: an empty
    /// table makes every run fall back to its standalone segmentation, so these cases
    /// exercise the per-run accounting alone.
    fn map(
        cleaned: &str,
        hs: usize,
        he: usize,
        evs: &[(usize, usize, i32, i32)],
    ) -> Vec<(i32, i32)> {
        map_cleaned_highlight_to_local(cleaned, hs, he, evs, &BlockScripts::default())
    }

    /// MANUAL-TEST 10.10b's fixture line. pulldown splits the fence into three `Text`
    /// events — `"Fence wrapping markup: ~~a "`, `"bold"` (inside `Strong`) and
    /// `" b~~ must"` — so neither `~~` has its partner in its own run. The claim (the
    /// whole fence, as the balancer widens it) must map onto `a bold b` and nothing
    /// else; scanning each run alone tagged the first and last events whole.
    #[test]
    fn a_claim_on_a_fence_split_across_events_maps_to_its_content_only() {
        let cleaned = "Fence wrapping markup: ~~a **bold** b~~ must";
        let scripts = BlockScripts::scan(cleaned);
        // Events (source → buffer): `Fence wrapping markup: ~~a ` 0..27 → 0..25,
        // `bold` 29..33 → 25..29, ` b~~ must` 35..44 → 29..36.
        let evs = [
            (0usize, 27usize, 0i32, 25i32),
            (29, 33, 25, 29),
            (35, 44, 29, 36),
        ];
        let (hs, he) = (cleaned.find("~~").unwrap(), cleaned.find(" must").unwrap());
        assert_eq!(
            map_cleaned_highlight_to_local(cleaned, hs, he, &evs, &scripts),
            vec![(23, 25), (25, 29), (29, 31)]
        );
        // And a claim on plain prose beside the fence stays exact too.
        let must = cleaned.find("must").unwrap();
        assert_eq!(
            map_cleaned_highlight_to_local(cleaned, must, cleaned.len(), &evs, &scripts),
            vec![(32, 36)]
        );
    }

    /// `a ==mark== b` (12 cleaned chars) renders as `a mark b` (8 buffer chars) —
    /// one content event whose length the scanner changed by stripping the `==` pairs.
    const STRIPPED: (&str, [(usize, usize, i32, i32); 1]) = ("a ==mark== b", [(0, 12, 0, 8)]);

    #[test]
    fn claim_on_a_plain_word_beside_a_stripped_construct_is_exact() {
        let (cleaned, evs) = STRIPPED;
        // cleaned 11..12 = the trailing "b" → buffer 7..8. Before the fix this
        // returned the WHOLE run (0, 8): the entire line washed for a one-char claim.
        assert_eq!(map(cleaned, 11, 12, &evs), vec![(7, 8)]);
        // …and the leading "a" at the other end of the run.
        assert_eq!(map(cleaned, 0, 1, &evs), vec![(0, 1)]);
    }

    #[test]
    fn claim_over_a_whole_construct_covers_its_content_only() {
        let (cleaned, evs) = STRIPPED;
        // cleaned 2..10 = `==mark==`; only `mark` reaches the buffer → 2..6.
        assert_eq!(map(cleaned, 2, 10, &evs), vec![(2, 6)]);
    }

    #[test]
    fn claim_inside_a_construct_maps_char_precisely() {
        let (cleaned, evs) = STRIPPED;
        // cleaned 5..7 = "ar" (content is `mark` at cleaned 4..8) → buffer 3..5, since
        // the buffer reads `a mark b`. (The balancer widens a real selection to the whole
        // construct; the mapper must still be exact within one.)
        assert_eq!(map(cleaned, 5, 7, &evs), vec![(3, 5)]);
    }

    #[test]
    fn a_claim_boundary_on_a_marker_clamps_to_the_kept_text() {
        let (cleaned, evs) = STRIPPED;
        // Starting inside the opening `==` (cleaned 3) has no buffer position of its
        // own — the marker is not there — so it clamps to where the markers began, and
        // the claim covers the kept text `mar` = buffer 2..5.
        assert_eq!(map(cleaned, 3, 7, &evs), vec![(2, 5)]);
    }

    #[test]
    fn every_scanned_construct_kind_maps_precisely() {
        // Each stripped run: 4 marker chars for the paired fences, 2 for the tight ones.
        for (cleaned, buf_len, claim, want) in [
            ("a ~~strike~~ b", 10, (13usize, 14usize), (9i32, 10i32)),
            ("a ^sup^ b", 7, (8, 9), (6, 7)),
            ("a ~sub~ b", 7, (8, 9), (6, 7)),
            ("a ==m== b", 5, (8, 9), (4, 5)),
        ] {
            let evs = [(0usize, cleaned.len(), 0i32, buf_len)];
            assert_eq!(
                map(cleaned, claim.0, claim.1, &evs),
                vec![want],
                "trailing-word claim in {cleaned:?}"
            );
        }
    }

    #[test]
    fn several_constructs_in_one_run_accumulate() {
        // `a ==m== b ^s^ c` (15) → `a m b s c` (9): 4 + 2 marker chars dropped.
        let cleaned = "a ==m== b ^s^ c";
        let evs = [(0usize, 15usize, 0i32, 9i32)];
        assert_eq!(map(cleaned, 14, 15, &evs), vec![(8, 9)]); // trailing "c"
        assert_eq!(map(cleaned, 8, 9, &evs), vec![(4, 5)]); // middle "b"
        assert_eq!(map(cleaned, 10, 13, &evs), vec![(6, 7)]); // `^s^` → "s"
    }

    #[test]
    fn a_genuinely_synthesised_run_is_still_tagged_whole() {
        // Smart punctuation (`--` → `–`) shrinks the run with no per-char
        // correspondence to recover, so the whole-event fallback must REMAIN for it —
        // tagging half a synthesised glyph would be worse than tagging the run.
        let cleaned = "a -- b";
        let evs = [(0usize, 6usize, 0i32, 5i32)];
        assert_eq!(map(cleaned, 5, 6, &evs), vec![(0, 5)]);
        // And a run carrying BOTH a construct and such a substitution cannot be fully
        // accounted for either, so it falls back rather than mis-mapping.
        let both = "a -- ==m== b";
        let evs_both = [(0usize, 12usize, 0i32, 7i32)];
        assert_eq!(map(both, 11, 12, &evs_both), vec![(0, 7)]);
    }

    #[test]
    fn a_one_to_one_run_is_unchanged() {
        // The pre-existing precise path: no constructs, lengths equal.
        let evs = [(0usize, 8usize, 0i32, 8i32)];
        assert_eq!(map("a mark b", 2, 6, &evs), vec![(2, 6)]);
        // Multi-byte text still counts CHARS, not bytes.
        let uni = "Åπ café";
        let evs_uni = [(0usize, uni.len(), 0i32, 7i32)];
        assert_eq!(
            map(uni, uni.find("café").unwrap(), uni.len(), &evs_uni),
            vec![(3, 7)]
        );
    }

    #[test]
    fn a_claim_spanning_two_events_maps_each() {
        // Two runs, the first marker-stripped: `a ==m== ` (8 chars → 4, dropping the
        // four `=`) then `tail` (4 → 4). Buffer reads `a m tail`.
        let cleaned = "a ==m== tail";
        let evs = [(0usize, 8usize, 0i32, 4i32), (8, 12, 4, 8)];
        assert_eq!(map(cleaned, 2, 12, &evs), vec![(2, 4), (4, 8)]);
    }
}

#[cfg(test)]
mod plain_claim_tests {
    use super::plain_claim;

    /// TDD 17.69: the page's reading of the claim, not the source's spelling of it.
    #[test]
    fn markdown_is_read_not_quoted() {
        assert_eq!(plain_claim("plain words"), "plain words");
        assert_eq!(
            plain_claim("a **bold** and *em* word"),
            "a bold and em word"
        );
        assert_eq!(plain_claim("[a link](https://x.test) here"), "a link here");
        assert_eq!(plain_claim("use `code` now"), "use code now");
        assert_eq!(plain_claim("![alt text](i.png) shown"), "alt text shown");
    }

    #[test]
    fn the_tight_constructs_lose_their_delimiters() {
        assert_eq!(plain_claim("a ==marked== b"), "a marked b");
        assert_eq!(plain_claim("x^2^ and H~2~O"), "x2 and H2O");
        // A fence whose halves sit in different events, as the page strips it.
        assert_eq!(plain_claim("~~a **bold** b~~ end"), "a bold b end");
    }

    #[test]
    fn a_line_break_reads_as_a_space() {
        assert_eq!(plain_claim("one\ntwo"), "one two");
    }
}
