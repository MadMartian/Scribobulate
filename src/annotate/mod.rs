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

pub(crate) mod anchored;
pub(crate) mod mutate;
pub(crate) mod scan;

pub(crate) use anchored::AnchoredSpan;
pub(crate) use mutate::{
    edit_comment, insert_or_extend_highlight, insert_point_comment, merged_comment_for,
    point_comment_anchor, remove_annotation,
};
pub(crate) use scan::{cleaned_to_original, extract, original_to_cleaned, AnnKind, Annotation};

/// Chars of `run[..upto]` that survive into the rendered buffer: everything except
/// the **delimiters** of the tight constructs the renderer strips (`==mark==`,
/// `~~strike~~`, `^sup^`, `~sub~` — `spans` comes from
/// [`crate::renderer::scan_script_spans`], the single definition of those). The
/// content between a construct's markers is kept and stays 1:1, so it maps
/// char-precisely; only the markers vanish.
///
/// An `upto` landing inside a delimiter run clamps to the last kept char before it,
/// which is what a claim boundary sitting on a marker means — the marker is not in
/// the buffer, so there is no position "inside" it to point at.
fn kept_chars(run: &str, spans: &[crate::renderer::ScriptSpan], upto: usize) -> i32 {
    let mut kept = 0i32;
    let mut at = 0usize; // byte cursor: start of the not-yet-counted plain text
    for sp in spans {
        if upto <= sp.outer.start {
            break;
        }
        kept += run[at..sp.outer.start].chars().count() as i32; // plain text, 1:1
        if upto <= sp.inner.start {
            return kept; // inside the OPENING delimiter — dropped
        }
        kept += run[sp.inner.start..sp.inner.end.min(upto)].chars().count() as i32; // content, 1:1
        if upto <= sp.outer.end {
            return kept; // inside the content (already counted) or the CLOSING delimiter
        }
        at = sp.outer.end;
    }
    kept + run[at..upto].chars().count() as i32
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
/// `spans` empty, `kept == run chars`) and a **marker-stripped** run
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
        let spans = crate::renderer::scan_script_spans(run);
        if kept_chars(run, &spans, run.len()) as usize == buf_len {
            out.push((
                before + kept_chars(run, &spans, ov_s - s),
                before + kept_chars(run, &spans, ov_e - s),
            ));
        } else {
            out.push((before, after));
        }
    }
    out
}

#[cfg(test)]
mod highlight_map_tests {
    use super::map_cleaned_highlight_to_local as map;

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
