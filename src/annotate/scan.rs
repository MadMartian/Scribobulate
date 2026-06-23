//! Part A (pure half) — the **pre-parse extraction** pass (researcher-sourced,
//! verified against pulldown-cmark 0.13.4; see ScrAP-122).
//!
//! CriticMarkup is not a pulldown-cmark extension, and its `~~ … ~>` substitution
//! delimiter collides with the tilde `scan_scripts` scanner. So we lift it out of
//! the source *before* pulldown ever sees it: [`extract`] returns
//! - `cleaned` — the source with CriticMarkup delimiters removed, ready to hand
//!   to `Parser::new` (highlighted/inserted/substituted text is kept so it still
//!   renders; standalone comments are removed entirely);
//! - `shifts` — a sorted `(cleaned_off, original_off)` table so any offset
//!   pulldown reports (which is in *cleaned* space) can be translated back to the
//!   original source the editor buffer holds (`cleaned_to_original`), keeping the
//!   render's source-map/copymap aligned with the editor for scroll-sync + copy;
//! - `annotations` — each construct with its **cleaned** content range (to apply
//!   the highlight tag / place the comment marker during render) and its
//!   **original** span + comment-body range (for the Remove/Edit mutations).
//!
//! Because deletion (not sentinel substitution) is used, offsets translate with a
//! pure piecewise-linear (slope-1) shift table — verified sound against
//! pulldown-cmark 0.13.4 (researcher findings). Block-spanning constructs are
//! **out of CriticMarkup spec** and are left as literal text (not extracted).

use super::mutate::{COMMENT_CLOSE, COMMENT_OPEN, HL_CLOSE, HL_OPEN};
use crate::span::{CleanedByteOffset, OriginalByteOffset};
use std::ops::Range;

const INS_OPEN: &str = "{++";
const INS_CLOSE: &str = "++}";
const DEL_OPEN: &str = "{--";
const DEL_CLOSE: &str = "--}";
const SUB_OPEN: &str = "{~~";
const SUB_CLOSE: &str = "~~}";
const SUB_ARROW: &str = "~>";

/// Every opener/closer pair the pass recognises, as ONE table.
///
/// It exists so that anything which must hold for *all* delimiters is written
/// once and a sixth pair inherits it without anyone remembering to. Two things
/// read it today: [`DelimIndex::NEEDLES`] (so a new closer is indexed rather
/// than scanned for) and the growth-ratio regression guard in the tests (so a
/// new pair is measured rather than assumed).
///
/// QA round 3 R3 is why: the first D-2 fix indexed the closers but left
/// `SUB_ARROW` on a bare `find`, and the regression guard that shipped with it
/// was written against one literal opener (`{==`) — so the guard could not see
/// the same-shape quadratic that survived under `{~~`. A guard whose pattern
/// cannot match the surviving instance is `ScrAP-220`; parameterising it over
/// this table is what stops the next delimiter from repeating it.
const PAIRS: [(&str, &str); 5] = [
    (HL_OPEN, HL_CLOSE),
    (COMMENT_OPEN, COMMENT_CLOSE),
    (INS_OPEN, INS_CLOSE),
    (DEL_OPEN, DEL_CLOSE),
    (SUB_OPEN, SUB_CLOSE),
];

/// Which CriticMarkup construct an [`Annotation`] is. v1 renders `Highlight` and
/// `Comment`; the suggested-edit kinds are recognised and their visible text is
/// kept, but no distinct styling is applied yet (deferred, not designed out —
/// keeping the inert kinds is what makes enabling them later a styling-only change).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum AnnKind {
    /// `{==content==}` — optionally immediately followed by `{>>comment<<}`.
    Highlight,
    /// `{>>comment<<}` — a standalone point comment (also the cross-block fallback).
    Comment,
    /// `{++text++}` — insertion (v1 inert).
    Insertion,
    /// `{--text--}` — deletion (v1 inert; text kept visible).
    Deletion,
    /// `{~~old~>new~~}` — substitution (v1 inert; `new` kept visible).
    Substitution,
}

/// One extracted CriticMarkup construct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Annotation {
    pub(crate) kind: AnnKind,
    /// Full construct span in the **original** source (drives Remove). Typed
    /// `OriginalByteOffset` (N1-phase-2) so it cannot be crossed with a cleaned
    /// offset in the merge arithmetic; `.raw()` at the `str`-index / marker-pipeline
    /// boundary.
    pub(crate) src_span: Range<OriginalByteOffset>,
    /// Range of the visible content in the **cleaned** text — the highlight span
    /// to tag, or (for a `Comment`) an empty range at the marker anchor. Typed
    /// `CleanedByteOffset` (N1-phase-2).
    pub(crate) cleaned_content: Range<CleanedByteOffset>,
    /// Comment-body byte range in the **original** source (drives Edit), if any.
    pub(crate) src_comment_body: Option<Range<usize>>,
    /// The extracted comment text (delimiters stripped), if any.
    pub(crate) comment: Option<String>,
}

impl Annotation {
    /// Whether this annotation is a **comment-bearing** annotation — the one and
    /// only definition of "is an annotation" shared by the preview's margin-chip
    /// builder (`preview::build::build_markers`) and the annotations viewer's list
    /// builder (`annotations::extract_entries`). Sharing one predicate is what keeps
    /// the two from drifting: if the list and the chips disagreed on membership, a
    /// viewer row could navigate to a chip that doesn't exist, or a chip could have
    /// no row (TDD 20.2).
    ///
    /// True for a `Highlight` that carries a comment (`{==claim==}{>>comment<<}`) and
    /// for a point `Comment` (`{>>comment<<}`). False for a bare `{==highlight==}` —
    /// a rendering feature, not an annotation, like bold — and for the v1-inert
    /// suggested-edit kinds (`{++ins++}`, `{--del--}`, `{~~a~>b~~}`), which have no
    /// chip and no defined presentation (deferred, not designed out).
    pub(crate) fn is_listed(&self) -> bool {
        matches!(self.kind, AnnKind::Highlight | AnnKind::Comment)
            && self.comment.is_some()
            && self.src_comment_body.is_some()
    }
}

/// The result of a pre-parse extraction pass over a Markdown source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Extraction {
    /// CriticMarkup-free source to hand to pulldown-cmark.
    pub(crate) cleaned: String,
    /// Annotations found, in source order.
    pub(crate) annotations: Vec<Annotation>,
    /// Sorted `(cleaned_off, original_off)` waypoints; see [`cleaned_to_original`].
    pub(crate) shifts: Vec<(usize, usize)>,
}

/// Translate a `cleaned` byte offset to its original-source byte offset using a
/// sorted shift table. The mapping is piecewise-linear with slope 1 (deletion
/// only), so within a segment `original = original_off + (cleaned_off - cleaned_seg)`.
pub(crate) fn cleaned_to_original(
    shifts: &[(usize, usize)],
    cleaned_off: CleanedByteOffset,
) -> OriginalByteOffset {
    // Boundary-only: unwrap to raw for the shift-table search, re-wrap the result.
    // The arithmetic below is byte-for-byte the pre-newtype code (N1).
    let cleaned_off = cleaned_off.raw();
    OriginalByteOffset::new(
        match shifts.binary_search_by(|&(c, _)| c.cmp(&cleaned_off)) {
            Ok(i) => shifts[i].1,
            Err(0) => cleaned_off,
            Err(i) => {
                let (c, o) = shifts[i - 1];
                o + (cleaned_off - c)
            }
        },
    )
}

/// Translate an **original**-source byte offset back to `cleaned` space — the
/// inverse of [`cleaned_to_original`], for the scroll-sync editor→preview
/// direction (Fork 2-B). The original column of the shift table is monotonic, so
/// we binary-search it; within a *deleted* region (where original advances but
/// cleaned does not) the result clamps to that segment's kept length, mapping the
/// whole removed span to its collapse point. Identity when `shifts == [(0,0)]`.
pub(crate) fn original_to_cleaned(
    shifts: &[(usize, usize)],
    orig_off: OriginalByteOffset,
) -> CleanedByteOffset {
    // Boundary-only (N1); arithmetic unchanged.
    let orig_off = orig_off.raw();
    let idx = shifts.partition_point(|&(_, o)| o <= orig_off);
    if idx == 0 {
        return CleanedByteOffset::new(orig_off);
    }
    let (c_i, o_i) = shifts[idx - 1];
    let kept = if idx < shifts.len() {
        shifts[idx].0 - c_i
    } else {
        usize::MAX
    };
    CleanedByteOffset::new(c_i + (orig_off - o_i).min(kept))
}

/// Parsed shape of one construct, used only within [`extract`].
struct Parsed {
    kind: AnnKind,
    span: Range<usize>,
    /// Original-source range of the visible text to KEEP in cleaned output;
    /// `None` for a standalone comment (nothing is kept).
    keep: Option<Range<usize>>,
    /// `(comment-body original range, extracted text)`, if the construct carries a
    /// comment.
    comment: Option<(Range<usize>, String)>,
}

/// Every fixed string [`extract`] has to locate, indexed ONCE per pass.
///
/// ## Why this exists (QA round 3, D-2)
///
/// `match_pair` used to answer "where is the next `==}`?" with
/// `source[after_open..].find(close)`, which scans to end-of-input when there is
/// no closer. `extract` calls it for **every** `{` in the document and advances
/// the cursor by one byte when the construct does not parse. On input that is
/// mostly unmatched openers that is a scan-to-EOF per opener: measured on this
/// tree, release build, a clean 4×-per-doubling curve — 64 KiB 82 ms, 128 KiB
/// 326 ms, 256 KiB 1.4 s, 512 KiB 5.3 s, extrapolating to well over a minute for
/// a few megabytes. It runs synchronously on the GTK main thread, so the window is
/// frozen for the duration of each run.
///
/// **Correcting the original write-up of this, which said "on every live-preview
/// keystroke".** That was wrong and it was mine. `window::livepreview` cancels any
/// pending re-render and reschedules on each buffer change, so the split-mode
/// re-render is **coalesced at 300 ms** — at most once per typing pause, and only
/// while the editor is visible. The freeze is real and a multi-second one is still
/// unacceptable; the per-keystroke amplifier attached to it was not measured and is
/// false. Severity claims about this path should be re-derived from "once per
/// re-render", not from a keystroke rate. [ANALYSED — source, `livepreview.rs:35-43`]
///
/// The delimiters are FIXED strings, so their positions are a property of the
/// source and not of where we happen to be looking from. Collecting them in one
/// linear pass turns every subsequent "next occurrence at or after `p`" into a
/// binary search, which makes the whole pass linearithmic regardless of how
/// adversarial the input is. Nothing is capped, refused or truncated — this is
/// the same answer computed a different way, which is why it needs no policy
/// decision and changes no output.
struct DelimIndex {
    /// Ascending positions of each needle, parallel to [`Self::NEEDLES`].
    needles: [Vec<usize>; Self::NEEDLES.len()],
    /// Ascending positions of every `\n\n` — the block break a CriticMarkup span
    /// may not cross. Indexed for the same reason: `content.contains("\n\n")` is
    /// O(content), and content can be the rest of the document.
    blank_lines: Vec<usize>,
}

impl DelimIndex {
    /// Every fixed string [`extract`] locates *inside* the document: each pair's
    /// closer, plus the substitution arrow. **Derived** from [`PAIRS`] rather
    /// than listed, so adding a sixth pair indexes its closer automatically —
    /// the R3 finding was precisely a delimiter that a hand-written list left on
    /// a bare `find`, and a hand-written list is a thing the next author must
    /// remember to extend.
    ///
    /// Openers are absent on purpose: they are only ever tested with
    /// `starts_with` at a known position, never searched for.
    const NEEDLES: [&'static str; PAIRS.len() + 1] = {
        let mut out = [SUB_ARROW; PAIRS.len() + 1];
        let mut i = 0;
        while i < PAIRS.len() {
            out[i] = PAIRS[i].1;
            i += 1;
        }
        out
    };

    fn build(source: &str) -> Self {
        let positions =
            |pat: &str| -> Vec<usize> { source.match_indices(pat).map(|(i, _)| i).collect() };
        DelimIndex {
            needles: Self::NEEDLES.map(positions),
            blank_lines: positions("\n\n"),
        }
    }

    /// Byte position of the first `needle` at or after `from`, or `None`.
    ///
    /// Panics if `needle` is not in [`Self::NEEDLES`], which is unreachable by
    /// construction and deliberately loud: returning `None` for an un-indexed
    /// needle would read as "not present in this document" and silently stop a
    /// whole construct kind from parsing.
    fn next(&self, needle: &str, from: usize) -> Option<usize> {
        let slot = Self::NEEDLES
            .iter()
            .position(|c| *c == needle)
            .expect("every searched-for delimiter is indexed in DelimIndex::NEEDLES");
        let v = &self.needles[slot];
        v.get(v.partition_point(|&p| p < from)).copied()
    }

    /// Byte position of the first `needle` lying WHOLLY within `range` — the
    /// indexed equivalent of `source[range].find(needle)`, in O(log n).
    ///
    /// The containment test is on the needle's END (`p + len <= range.end`), not
    /// its start, so an occurrence straddling the closing delimiter is rejected
    /// exactly as a slice-local `find` would never have seen it.
    fn next_within(&self, needle: &str, range: &Range<usize>) -> Option<usize> {
        let p = self.next(needle, range.start)?;
        (p + needle.len() <= range.end).then_some(p)
    }

    /// Whether `range` contains a `\n\n`. Equivalent to
    /// `source[range].contains("\n\n")`, in O(log n) rather than O(len).
    ///
    /// A `\n\n` starting at `p` lies within `[start, end)` iff `p + 2 <= end`,
    /// so the guard is on `p + 2`, not on `p` — an off-by-one here would reject
    /// a legal span whose content merely ENDS with a newline.
    fn crosses_block(&self, range: &Range<usize>) -> bool {
        let i = self.blank_lines.partition_point(|&p| p < range.start);
        self.blank_lines.get(i).is_some_and(|&p| p + 2 <= range.end)
    }
}

/// If `source[i..]` opens with `open`, return the delimited content's original
/// range and the byte index just past `close`. Rejects a block-crossing span —
/// the reference spec says a CriticMarkup span "must be contained within a
/// single block", so such input is left literal, matching Lezer and the
/// reference regex.
fn match_pair(
    source: &str,
    idx: &DelimIndex,
    i: usize,
    open: &str,
    close: &str,
) -> Option<(Range<usize>, usize)> {
    if !source[i..].starts_with(open) {
        return None;
    }
    let after_open = i + open.len();
    let close_at = idx.next(close, after_open)?;
    let content = after_open..close_at;
    if idx.crosses_block(&content) {
        return None;
    }
    Some((content, close_at + close.len()))
}

/// Attempt to parse a construct at `i` (which points at a `{`). Returns `None`
/// when the bytes at `i` are not a well-formed CriticMarkup construct — the caller
/// then treats the `{` as literal text.
fn try_parse(source: &str, idx: &DelimIndex, i: usize) -> Option<Parsed> {
    let rest = &source[i..];

    if rest.starts_with(HL_OPEN) {
        let (content, mut end) = match_pair(source, idx, i, HL_OPEN, HL_CLOSE)?;
        // An immediately-adjacent {>>…<<} binds to the highlight as its comment.
        let mut comment = None;
        if source[end..].starts_with(COMMENT_OPEN) {
            if let Some((body, cend)) = match_pair(source, idx, end, COMMENT_OPEN, COMMENT_CLOSE) {
                comment = Some((body.clone(), source[body].to_string()));
                end = cend;
            }
        }
        return Some(Parsed {
            kind: AnnKind::Highlight,
            span: i..end,
            keep: Some(content),
            comment,
        });
    }

    if rest.starts_with(COMMENT_OPEN) {
        let (body, end) = match_pair(source, idx, i, COMMENT_OPEN, COMMENT_CLOSE)?;
        return Some(Parsed {
            kind: AnnKind::Comment,
            span: i..end,
            keep: None,
            comment: Some((body.clone(), source[body].to_string())),
        });
    }

    if rest.starts_with(INS_OPEN) {
        let (content, end) = match_pair(source, idx, i, INS_OPEN, INS_CLOSE)?;
        return Some(Parsed {
            kind: AnnKind::Insertion,
            span: i..end,
            keep: Some(content),
            comment: None,
        });
    }

    if rest.starts_with(DEL_OPEN) {
        let (content, end) = match_pair(source, idx, i, DEL_OPEN, DEL_CLOSE)?;
        return Some(Parsed {
            kind: AnnKind::Deletion,
            span: i..end,
            keep: Some(content),
            comment: None,
        });
    }

    if rest.starts_with(SUB_OPEN) {
        let (content, end) = match_pair(source, idx, i, SUB_OPEN, SUB_CLOSE)?;
        // Substitution requires `~>`; keep only the "new" side visible in v1.
        //
        // Indexed, not `source[content].find(SUB_ARROW)`. The `?` on a MISSING
        // arrow is what made the scan quadratic: the construct is rejected, the
        // cursor advances ONE byte, and the next `{~~` re-scans the same content
        // to the same closer. `{~~` repeated with a single trailing `~~}` is
        // therefore ~n²/6 — the same shape, in the same function, on the same
        // main thread, as the closers' scan-to-EOF (QA round 3 D-2, R3).
        let arrow_at = idx.next_within(SUB_ARROW, &content)?;
        let new_range = arrow_at + SUB_ARROW.len()..content.end;
        return Some(Parsed {
            kind: AnnKind::Substitution,
            span: i..end,
            keep: Some(new_range),
            comment: None,
        });
    }

    None
}

/// Run the pre-parse extraction pass. See the module docs.
pub(crate) fn extract(source: &str) -> Extraction {
    let mut cleaned = String::with_capacity(source.len());
    let mut shifts: Vec<(usize, usize)> = vec![(0, 0)];
    let mut annotations: Vec<Annotation> = Vec::new();
    let mut cursor = 0usize;
    // One linear pass over the source up front; every delimiter lookup below is
    // then a binary search rather than a scan to end-of-input (see `DelimIndex`).
    let idx = DelimIndex::build(source);

    while let Some(rel) = source[cursor..].find('{') {
        let i = cursor + rel;
        let Some(p) = try_parse(source, &idx, i) else {
            // Not a construct: keep everything through this `{` as literal text.
            cleaned.push_str(&source[cursor..=i]);
            cursor = i + 1;
            continue;
        };

        // Literal text before the construct is kept verbatim (no shift).
        cleaned.push_str(&source[cursor..i]);

        let cleaned_content = match &p.keep {
            Some(keep) => {
                // Keep the visible text; record the shift at its start and end.
                let start = cleaned.len();
                shifts.push((start, keep.start));
                cleaned.push_str(&source[keep.clone()]);
                let end = cleaned.len();
                shifts.push((end, p.span.end));
                CleanedByteOffset::new(start)..CleanedByteOffset::new(end)
            }
            None => {
                // Standalone comment: nothing kept; marker anchors here.
                let anchor = cleaned.len();
                shifts.push((anchor, p.span.end));
                CleanedByteOffset::new(anchor)..CleanedByteOffset::new(anchor)
            }
        };

        annotations.push(Annotation {
            kind: p.kind,
            src_span: OriginalByteOffset::new(p.span.start)..OriginalByteOffset::new(p.span.end),
            cleaned_content,
            src_comment_body: p.comment.as_ref().map(|(r, _)| r.clone()),
            comment: p.comment.map(|(_, t)| t),
        });
        cursor = p.span.end;
    }
    cleaned.push_str(&source[cursor..]);

    Extraction {
        cleaned,
        annotations,
        shifts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- cleaned output ---------------------------------------------------

    #[test]
    fn highlight_comment_strips_to_visible_claim() {
        let e = extract("the earth is {==flat==}{>>citation needed<<} ok");
        assert_eq!(e.cleaned, "the earth is flat ok");
    }

    #[test]
    fn standalone_comment_is_removed_entirely() {
        let e = extract("a claim{>>who says?<<} continues");
        assert_eq!(e.cleaned, "a claim continues");
    }

    #[test]
    fn no_criticmarkup_is_returned_verbatim() {
        let e = extract("plain text with { a brace but no marker");
        assert_eq!(e.cleaned, "plain text with { a brace but no marker");
        assert!(e.annotations.is_empty());
    }

    #[test]
    fn suggestions_keep_their_visible_text_inert() {
        assert_eq!(extract("say {++hello++} there").cleaned, "say hello there");
        assert_eq!(extract("the {--old--} way").cleaned, "the old way");
        assert_eq!(extract("go {~~left~>right~~} now").cleaned, "go right now");
    }

    // ---- annotation metadata ---------------------------------------------

    #[test]
    fn highlight_records_kind_span_and_comment() {
        let src = "x {==claim==}{>>note<<} y";
        let e = extract(src);
        assert_eq!(e.annotations.len(), 1);
        let a = &e.annotations[0];
        assert_eq!(a.kind, AnnKind::Highlight);
        // src_span covers the whole {==…==}{>>…<<} construct.
        assert_eq!(
            &src[a.src_span.start.raw()..a.src_span.end.raw()],
            "{==claim==}{>>note<<}"
        );
        assert_eq!(a.comment.as_deref(), Some("note"));
        // comment body maps back to the original "note".
        assert_eq!(&src[a.src_comment_body.clone().unwrap()], "note");
        // cleaned_content indexes "claim" in the cleaned text.
        assert_eq!(
            &e.cleaned[a.cleaned_content.start.raw()..a.cleaned_content.end.raw()],
            "claim"
        );
    }

    #[test]
    fn highlight_without_comment_is_supported() {
        let e = extract("a {==plain highlight==} b");
        let a = &e.annotations[0];
        assert_eq!(a.kind, AnnKind::Highlight);
        assert_eq!(a.comment, None);
        assert_eq!(
            &e.cleaned[a.cleaned_content.start.raw()..a.cleaned_content.end.raw()],
            "plain highlight"
        );
    }

    #[test]
    fn standalone_comment_anchor_is_empty_at_position() {
        let e = extract("claim{>>note<<}!");
        let a = &e.annotations[0];
        assert_eq!(a.kind, AnnKind::Comment);
        assert!(a.cleaned_content.is_empty());
        // anchored right after "claim" (5 chars).
        assert_eq!(a.cleaned_content.start.raw(), "claim".len());
        assert_eq!(a.comment.as_deref(), Some("note"));
    }

    // ---- offset translation ----------------------------------------------

    #[test]
    fn cleaned_offsets_translate_back_to_original() {
        let src = "the earth is {==flat==}{>>note<<} ok";
        let e = extract(src);
        // "flat" starts at cleaned offset 13; in the original it starts after
        // "the earth is {==" = 16.
        let cs = e.cleaned.find("flat").unwrap();
        assert_eq!(cs, 13);
        assert_eq!(
            cleaned_to_original(&e.shifts, CleanedByteOffset::new(cs)).raw(),
            src.find("flat").unwrap()
        );
        // The " ok" after the whole construct maps past the removed tail.
        let cok = e.cleaned.find(" ok").unwrap();
        let ook = cleaned_to_original(&e.shifts, CleanedByteOffset::new(cok)).raw();
        assert_eq!(&src[ook..ook + 3], " ok");
    }

    #[test]
    fn multiple_annotations_translate_independently() {
        let src = "{==a==}{>>c1<<} mid {++ins++} end{>>c2<<}";
        let e = extract(src);
        assert_eq!(e.cleaned, "a mid ins end");
        // Every kept char translates back to the same char in the original.
        for (ci, ch) in e.cleaned.char_indices() {
            let oi = cleaned_to_original(&e.shifts, CleanedByteOffset::new(ci)).raw();
            assert_eq!(
                src[oi..].chars().next(),
                Some(ch),
                "mismatch at cleaned {ci}"
            );
        }
    }

    #[test]
    fn original_to_cleaned_is_the_clamped_inverse() {
        // "the earth is flat ok" from "the earth is {==flat==}{>>note<<} ok".
        let shifts = [(0usize, 0usize), (13, 16), (17, 33)];
        let o2c = |o: usize| original_to_cleaned(&shifts, OriginalByteOffset::new(o)).raw();
        assert_eq!(o2c(16), 13); // "flat" start
        assert_eq!(o2c(20), 17); // end of "flat"
        assert_eq!(o2c(33), 17); // removed span collapses to 17
        assert_eq!(o2c(5), 5); // within leading kept text
        for co in [0usize, 5, 13, 16, 17, 20] {
            let oo = cleaned_to_original(&shifts, CleanedByteOffset::new(co)).raw();
            assert_eq!(o2c(oo), co, "round-trip at cleaned {co}");
        }
        // identity, un-annotated
        assert_eq!(
            original_to_cleaned(&[(0, 0)], OriginalByteOffset::new(42)).raw(),
            42
        );
    }

    // ---- robustness -------------------------------------------------------

    #[test]
    fn unclosed_delimiters_stay_literal() {
        let e = extract("open {== but never closed");
        assert_eq!(e.cleaned, "open {== but never closed");
        assert!(e.annotations.is_empty());
    }

    #[test]
    fn block_spanning_highlight_is_left_literal() {
        // A {== whose ==} crosses a blank line is out of spec → not extracted.
        let src = "{==first para\n\nsecond para==}";
        let e = extract(src);
        assert_eq!(e.cleaned, src);
        assert!(e.annotations.is_empty());
    }

    #[test]
    fn multibyte_content_extracts_correctly() {
        let src = "Åπ {==café==}{>>délicieux<<}";
        let e = extract(src);
        assert_eq!(e.cleaned, "Åπ café");
        let a = &e.annotations[0];
        assert_eq!(
            &e.cleaned[a.cleaned_content.start.raw()..a.cleaned_content.end.raw()],
            "café"
        );
        assert_eq!(a.comment.as_deref(), Some("délicieux"));
        assert_eq!(&src[a.src_comment_body.clone().unwrap()], "délicieux");
    }

    #[test]
    fn adjacent_constructs_do_not_merge_or_drop() {
        let src = "{==a==}{>>1<<}{==b==}{>>2<<}";
        let e = extract(src);
        assert_eq!(e.cleaned, "ab");
        assert_eq!(e.annotations.len(), 2);
        assert_eq!(e.annotations[0].comment.as_deref(), Some("1"));
        assert_eq!(e.annotations[1].comment.as_deref(), Some("2"));
        assert_eq!(
            &e.cleaned[e.annotations[1].cleaned_content.start.raw()
                ..e.annotations[1].cleaned_content.end.raw()],
            "b"
        );
    }

    // ── input-limit regressions (QA round 3, D-2) ─────────────────────────────

    /// The extraction pass must stay linear in the size of an ADVERSARIAL
    /// document, not just a well-formed one.
    ///
    /// `match_pair` used to answer "where is the closing `==}`?" by scanning to
    /// end-of-input, and `extract` asks that question once per `{` while
    /// advancing one byte at a time when the construct does not parse. Measured
    /// before the fix (release build): 64 KiB 82 ms, 128 KiB 326 ms, 256 KiB
    /// 1.40 s, 512 KiB 5.27 s — textbook 4×-per-doubling, extrapolating to well
    /// over a minute at a few megabytes, synchronously on the GTK main thread, on
    /// open and on each split-mode re-render (which is 300 ms-coalesced, NOT
    /// per-keystroke — see `DelimIndex`'s note correcting that claim).
    ///
    /// After indexing the delimiters once per pass: 64 KiB 0.3 ms, 512 KiB
    /// 2.1 ms, 2 MiB 8.5 ms (release) / 92 ms (debug) — 2× per doubling.
    ///
    /// **Asserted as a RATIO, not a wall-clock bound.** A machine-speed
    /// threshold is either flaky on a loaded CI box or so generous it stops
    /// discriminating; the growth exponent is the property that actually
    /// regressed, and it is machine-independent. Quadratic would be ~16× for 4×
    /// the input, linear ~4×; the assertion allows up to 8×, which no linear
    /// implementation approaches and no quadratic one survives. The base input
    /// is large enough (256 KiB) that timer granularity is not a factor.
    ///
    /// **Each sample is a best-of-N**, because a ratio between two single wall-clock
    /// draws is only as stable as the noisier of the two — see `time_extract` below.
    /// A ratio being machine-INDEPENDENT does not make it load-independent.
    ///
    /// **Parameterised over [`PAIRS`], and over two input SHAPES, because the
    /// first version of this guard was not** (`ScrAP-220`). It built its
    /// input as `"{==".repeat(n)` — the delimiter that had just been fixed —
    /// while an identical quadratic survived under `{~~`, whose arrow scan is
    /// only reached when a closer *is* present. The guard passed with the defect
    /// fully intact: not a weak assertion, a pattern that could not match the
    /// surviving instance. Both dimensions matter, so both are enumerated rather
    /// than chosen:
    ///
    /// * **every pair**, so a sixth delimiter added to [`PAIRS`] is measured
    ///   without anyone remembering to extend this test;
    /// * **openers alone** (no closer — the closer lookup runs to EOF) and
    ///   **openers with one trailing closer** (the closer resolves, so parsing
    ///   proceeds *into* the construct body and any scan there is reached).
    #[test]
    fn extraction_over_adversarial_input_grows_linearly_not_quadratically() {
        /// How many times each input is timed before the best result is kept.
        const TIMING_SAMPLES: usize = 5;

        /// Absolute ceiling for the 1024 KiB input. The post-fix debug measurement is
        /// ~45 ms and the PRE-fix one was 96 SECONDS, so this sits ~66x above the former
        /// and ~30x below the latter — it discriminates without being sensitive to
        /// machine speed. Defined once because `time_extract` and the assertion below
        /// must agree on it: the sampler uses it to stop early, and the assertion uses it
        /// to fail, so two copies could disagree about what "too slow" means.
        const LINEAR_CEILING: std::time::Duration = std::time::Duration::from_millis(3000);

        /// Best-of-[`TIMING_SAMPLES`] cost of one `extract` pass.
        ///
        /// The **minimum**, not a single sample and not a mean, because timing noise on
        /// a shared machine is strictly ADDITIVE: preemption, cache eviction and
        /// frequency scaling can only make a run slower than the work actually costs,
        /// never faster. The floor of the observed distribution is therefore the
        /// estimate of the noise-free cost, while one sample is a single arbitrary draw
        /// from a right-skewed one — and a mean would fold the outliers back in, which
        /// is precisely what a ratio between two samples must not do. It also discards
        /// first-call warm-up for free, since later iterations run warm.
        ///
        /// The ratio assertion below is what makes this necessary. A slow draw on
        /// `large` inflates the ratio with no algorithmic change at all, and that
        /// reddened a full parallel test run (~1 in 5, under a concurrent release
        /// build) with the code correct. The absolute ceiling never flaked; only the
        /// ratio did. Sampling is the fix rather than a looser threshold, because
        /// widening the threshold trades the flake for exactly the discriminating power
        /// the guard exists to have.
        fn time_extract(src: &str) -> std::time::Duration {
            let mut best = std::time::Duration::MAX;
            for _ in 0..TIMING_SAMPLES {
                let t = std::time::Instant::now();
                let e = extract(src);
                // Consume the result so nothing can be optimised away. Only the
                // no-closer shape has a pinnable output (see the caller); the
                // with-closer shape's output differs per pair, so all this asserts
                // is that extraction stays a lossless-or-shrinking transform.
                assert!(e.cleaned.len() <= src.len());
                assert!(e.annotations.len() <= 1);
                best = best.min(t.elapsed());
                // Stop as soon as one sample is already past the ceiling. Sampling must
                // not make the FAILURE mode more expensive than the bug: the regression
                // this guards against costs ~96 s per call, so a plain best-of-5 over
                // five pairs x four measurements turns a red run into a multi-hour one
                // (measured — the first attempt at this fix had to be killed). The early
                // exit is sound only because the ceiling has ~66x headroom over the
                // linear cost: additive noise cannot lift a 45 ms pass past 3 s, so a
                // sample over it means a real regression, not a slow draw. Do not reuse
                // this shape for a tight bound, where it WOULD reintroduce the flake.
                if best > LINEAR_CEILING {
                    break;
                }
            }
            best
        }

        for (open, close) in PAIRS {
            // Shape A: openers only. The closer is never found, so the failure
            // is in the closer lookup itself.
            let repeats = |kib: usize| open.repeat(kib * 1024 / open.len());
            let bare_small = time_extract(&repeats(256));
            let bare_large = time_extract(&repeats(1024)); // 4x the input

            // Shape B: the same openers with ONE closer at the end. Now the
            // closer resolves for every opener, so each one is parsed further —
            // this is the shape that reaches `{~~`'s arrow scan, and the shape
            // the original guard had no analogue of.
            let closed = |kib: usize| repeats(kib) + close;
            let closed_small = time_extract(&closed(256));
            let closed_large = time_extract(&closed(1024));

            for (shape, small, large) in [
                ("openers only", bare_small, bare_large),
                ("openers + one closer", closed_small, closed_large),
            ] {
                let ratio = large.as_secs_f64() / small.as_secs_f64().max(1e-9);
                assert!(
                    ratio < 8.0,
                    "`{open}` … `{close}` ({shape}): extraction time grew \
                     {ratio:.1}x for 4x the input ({small:?} -> {large:?}). \
                     Linear is ~4x, quadratic ~16x — this looks like the \
                     scan-per-opener regression (QA R3 D-2) has come back."
                );
                // A ratio alone can be defeated by a CONSTANT-FACTOR speedup on
                // a still quadratic algorithm (qa's point): make it 4x faster
                // and the ratio drifts under the threshold while the exponent
                // stands. So pair it with an absolute ceiling that only a linear
                // implementation can meet (`LINEAR_CEILING`, above, which the
                // sampler shares).
                assert!(
                    large < LINEAR_CEILING,
                    "`{open}` … `{close}` ({shape}): {large:?} for 1024 KiB is \
                     far past any linear implementation's cost — the growth \
                     ratio may have been masked by a constant-factor speedup on \
                     a still-quadratic algorithm."
                );
            }

            // The no-closer shape has one exact output, so pin it: unmatched
            // openers parse as nothing and survive verbatim into cleaned text.
            let src = repeats(4);
            let e = extract(&src);
            assert_eq!(e.annotations.len(), 0, "`{open}` alone parses nothing");
            assert_eq!(e.cleaned, src, "`{open}` alone survives verbatim");
        }
    }

    /// The delimiter index must be a faithful re-implementation of the scans it
    /// replaced, including the boundary the old `contains("\\n\\n")` got right by
    /// construction: a span whose content merely ENDS with a newline does not
    /// cross a block, but one containing a blank line does.
    #[test]
    fn the_block_break_boundary_is_unchanged_by_the_index() {
        // Content ends with a single newline — legal, still one block.
        let e = extract("{==a\n==}");
        assert_eq!(
            e.annotations.len(),
            1,
            "a trailing newline is not a block break"
        );

        // Content contains a blank line — out of spec, left literal.
        let src = "{==a\n\nb==}";
        let e = extract(src);
        assert_eq!(e.annotations.len(), 0, "a blank line ends the construct");
        assert_eq!(e.cleaned, src, "a rejected construct survives verbatim");
    }
}
