//! Part B — pure source-mutation: write / remove / edit a CriticMarkup
//! annotation in a Markdown source string (the pure mutation core).
//!
//! Every function here is a pure `&str -> String` transform over **byte offsets**
//! the caller supplies (from the preview-buffer → source map). None of them
//! parse; they perform balanced string surgery and are the inverse of the
//! scanner. The one non-mechanical concern is that user-typed comment text must
//! never be able to *close its own token* — see [`sanitize_comment`].

use crate::span::{CleanedByteOffset, OriginalByteOffset};
use std::ops::Range;

/// The CriticMarkup delimiters, named once so the scanner and the mutator agree.
pub(crate) const HL_OPEN: &str = "{==";
pub(crate) const HL_CLOSE: &str = "==}";
pub(crate) const COMMENT_OPEN: &str = "{>>";
pub(crate) const COMMENT_CLOSE: &str = "<<}";

/// Neutralise any sequence in user comment text that would prematurely close the
/// comment token (`<<}`). CriticMarkup defines no escape mechanism, so we defuse
/// the only dangerous sequence by wedging a space into it (`<<}` → `<< }`); the
/// scanner then never sees a false close. Outer whitespace is trimmed so the
/// stored form is `{>>comment<<}`, not `{>> comment <<}`.
///
/// The highlighted *content* is deliberately NOT sanitised — it is verbatim
/// source the caller chose, and guarding a selection that itself contains `==}`
/// is the caller's job (offer no annotation over existing CriticMarkup).
///
/// Spec scope (F-SEC-001): this targets the **CriticMarkup v1** grammar as
/// implemented by this crate's scanner — the four delimiters `{==`/`==}` and
/// `{>>`/`<<}` (see [`HL_OPEN`]/[`COMMENT_CLOSE`]). `<<}` is the *only* sequence a
/// comment body can use to close its own token, so neutralising it is complete for
/// that grammar. If the scanner ever grows additional comment constructs (nested
/// or alternative close markers), this single-sequence defuse must be revisited in
/// lockstep — it is not a general-purpose sanitiser.
pub(crate) fn sanitize_comment(comment: &str) -> String {
    comment.trim().replace(COMMENT_CLOSE, "<< }")
}

/// Insert a highlight+comment annotation wrapping `range` in `source`:
/// `{==<source[range]>==}{>>comment<<}`. `range` must be valid char-boundary byte
/// offsets into `source` (as produced by the buffer→source map).
pub(crate) fn insert_highlight_comment(source: &str, range: Range<usize>, comment: &str) -> String {
    // Total, like `remove_annotation`/`edit_comment` next door (QA round 3, P-6).
    // The doc comment states the range must be valid char-boundary offsets, and
    // the mapping that produces it is believed to honour that — but the two
    // REMOVAL paths in this same file already refuse to rely on the equivalent
    // belief, with a test explaining why, and this one relied on it. The
    // asymmetry is the finding: two functions with the same kind of input,
    // reaching opposite conclusions about whether to trust it. A panic here is a
    // process abort (no `catch_unwind` on the app path), so the cost of being
    // wrong is not an error dialog.
    //
    // An unusable range yields the source UNCHANGED, which is the honest
    // degradation: the annotation is not inserted, nothing is corrupted, and the
    // user's document is exactly as they left it.
    let (Some(before), Some(inner), Some(after)) = (
        source.get(..range.start),
        source.get(range.clone()),
        source.get(range.end..),
    ) else {
        log::warn!(
            "refusing to insert an annotation over an unusable range {range:?} \
             (source is {} bytes)",
            source.len()
        );
        return source.to_string();
    };
    let mut out = String::with_capacity(source.len() + comment.len() + 12);
    out.push_str(before);
    out.push_str(HL_OPEN);
    out.push_str(inner);
    out.push_str(HL_CLOSE);
    out.push_str(COMMENT_OPEN);
    out.push_str(&sanitize_comment(comment));
    out.push_str(COMMENT_CLOSE);
    out.push_str(after);
    out
}

/// The safe insertion offset for a cross-block point comment
/// whose anchor `at` is the selection END mapped cleaned→original.
///
/// A multi-block selection cannot wrap as a highlight, so it drops a bare `{>>…<<}` at
/// its end (D5). But that end can map *inside* an existing CriticMarkup construct — e.g.
/// between `{==fl` and `at==}` when the selection ended part-way through an
/// already-highlighted claim. Splicing a bare `{>>comment<<}` there yields malformed
/// nesting (`{==fl{>>comment<<}at==}`) that the scanner re-reads as the comment being
/// *part of the claim text*, silently swallowing the user's comment and adding no
/// annotation (the multi-block-over-existing-annotation defect — proven candidate (b),
/// "splice corrupts", over candidate (a), "resolver bails").
///
/// A point comment deliberately **does not extend** an existing annotation — extending is
/// the intra-block highlight path's job (`insert_or_extend_highlight`), and a multi-block
/// selection was never wired to it. So the fix is not to merge
/// but to land cleanly *outside*: if `at` falls strictly within an extracted construct's
/// span, snap it to that construct's END, attaching the comment immediately AFTER the whole
/// `{==…==}{>>…<<}` as its own standalone `{>>comment<<}`. Constructs never overlap, so at
/// most one contains `at`, and its end is a clean boundary (at worst the start of the next
/// construct, never strictly inside it) — a single pass suffices.
pub(crate) fn point_comment_anchor(source: &str, at: usize) -> usize {
    super::extract(source)
        .annotations
        .iter()
        .find(|a| a.src_span.start.raw() < at && at < a.src_span.end.raw())
        .map_or(at, |a| a.src_span.end.raw())
}

/// Insert a bare point comment `{>>comment<<}` at byte offset `at` in `source`
/// (the cross-block fallback — anchored at the selection END).
/// Callers apply [`point_comment_anchor`] to `at` first, so the splice never lands inside
/// an existing construct.
pub(crate) fn insert_point_comment(source: &str, at: usize, comment: &str) -> String {
    // Total for the same reason as `insert_highlight_comment` above (P-6).
    let (Some(before), Some(after)) = (source.get(..at), source.get(at..)) else {
        log::warn!(
            "refusing to insert a point comment at an unusable offset {at} \
             (source is {} bytes)",
            source.len()
        );
        return source.to_string();
    };
    let mut out = String::with_capacity(source.len() + comment.len() + 6);
    out.push_str(before);
    out.push_str(COMMENT_OPEN);
    out.push_str(&sanitize_comment(comment));
    out.push_str(COMMENT_CLOSE);
    out.push_str(after);
    out
}

/// Remove the annotation construct occupying `span` in `source`. For a
/// highlight+comment, pass `keep_inner` = the byte range of the highlighted text
/// (a sub-range of `span`, in original-source coordinates) so the claim survives
/// un-annotated; for a point comment pass `None` to delete the whole construct.
///
/// This is the exact inverse of the insert functions: it is the mechanism behind
/// the popover's **Remove**.
pub(crate) fn remove_annotation(
    source: &str,
    span: Range<usize>,
    keep_inner: Option<Range<usize>>,
) -> Option<String> {
    // TOTAL, deliberately. Every range reaching here was computed against some
    // earlier state of the document, so an invalid one is a reachable state rather
    // than a bug to assert on: raw slicing panicked out of bounds (and a panic in a
    // GTK handler aborts the process), and in-bounds-but-wrong silently destroyed
    // unrelated text. `get` gives bounds AND char-boundary checking in one, and
    // `None` gives the caller a clean no-op. See `AnchoredSpan` for how callers
    // avoid ever presenting a stale range in the first place; this is the floor
    // under that, not a substitute for it.
    let head = source.get(..span.start)?;
    let tail = source.get(span.end..)?;
    let kept = match keep_inner {
        // The kept content must lie WITHIN the construct being removed. A range
        // outside it would splice in text from somewhere else entirely.
        Some(inner) if inner.start < span.start || inner.end > span.end => return None,
        Some(inner) => Some(source.get(inner)?),
        None => None,
    };
    let mut out = String::with_capacity(source.len());
    out.push_str(head);
    if let Some(kept) = kept {
        out.push_str(kept);
    }
    out.push_str(tail);
    Some(out)
}

/// Replace the comment body (the text strictly between `{>>` and `<<}`, given as
/// the byte range `comment_body`) with `new_comment`. Backs the popover's
/// **Edit**. The new text is sanitised identically to insert.
pub(crate) fn edit_comment(
    source: &str,
    comment_body: Range<usize>,
    new_comment: &str,
) -> Option<String> {
    // Total for the same reason as `remove_annotation` — see the note there.
    let head = source.get(..comment_body.start)?;
    let tail = source.get(comment_body.end..)?;
    let mut out = String::with_capacity(source.len() + new_comment.len());
    out.push_str(head);
    out.push_str(&sanitize_comment(new_comment));
    out.push_str(tail);
    Some(out)
}

/// Separator joining the comments of several annotations a selection will merge.
/// Safe to round-trip through `{>>…<<}`: `sanitize_comment`
/// rewrites only `<<}`, and no CriticMarkup delimiter (`{==` `==}` `{>>` `<<}` `{++` `++}`
/// `{--` `--}` `{~~` `~~}`) contains a `|`, so the joined string re-extracts as ONE
/// comment with the bar intact. Asserted, not assumed — see
/// `a_joined_comment_round_trips_as_one_comment`.
pub(crate) const COMMENT_JOIN: &str = " | ";

/// The Highlight annotations whose content intersects the **cleaned-space** selection
/// `sel_cs..sel_ce`.
///
/// This is THE definition of "overlapping". It is shared by
/// [`insert_or_extend_highlight`], which merges these annotations and drops their comments,
/// and by [`merged_comment_for`], which pre-populates the comment card from them — so the
/// entry showing the user what is about to be merged and the mutation doing the merging can
/// never disagree. A second predicate written to match this one would be free to drift, and
/// the disagreement would be silent and document-dependent.
///
/// Half-open intersection: touching at an endpoint does not overlap.
pub(crate) fn intersecting_highlights(
    annotations: &[super::Annotation],
    sel_cs: CleanedByteOffset,
    sel_ce: CleanedByteOffset,
) -> Vec<&super::Annotation> {
    annotations
        .iter()
        .filter(|a| {
            a.kind == super::AnnKind::Highlight
                && a.cleaned_content.start < sel_ce
                && sel_cs < a.cleaned_content.end
        })
        .collect()
}

/// The existing comments a new annotation over `range` (original-source bytes) would
/// merge, joined in **source order** with [`COMMENT_JOIN`]; `None` when it would merge
/// nothing.
///
/// `insert_or_extend_highlight` replaces every intersecting construct —
/// including its `{>>comment<<}` — with the union carrying the NEW comment, so those
/// comments are destroyed. The mutation is right to do so (extend, don't nest); the defect
/// was that the user never saw it coming. Pre-populating the card with exactly what is
/// about to be overwritten turns a silent destructive merge into an explicit edit: the user
/// sees the prior text, edits or prunes it, and what they see is what is written back.
///
/// `extract` yields annotations in document order, so the join is in source order.
pub(crate) fn merged_comment_for(source: &str, range: Range<usize>) -> Option<String> {
    let ext = super::extract(source);
    // N1-phase-2: the selection is carried in typed CLEANED space through the whole
    // merge, so it cannot be crossed with an original offset (the recurring bug class).
    let sel_cs = super::original_to_cleaned(&ext.shifts, OriginalByteOffset::new(range.start));
    let sel_ce = super::original_to_cleaned(&ext.shifts, OriginalByteOffset::new(range.end));
    let joined = intersecting_highlights(&ext.annotations, sel_cs, sel_ce)
        .iter()
        .filter_map(|a| a.comment.as_deref())
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .collect::<Vec<_>>()
        .join(COMMENT_JOIN);
    (!joined.is_empty()).then_some(joined)
}

/// Create a highlight+comment over `range` — UNLESS `range` intersects one or more
/// existing highlight annotations, in which case this is treated as an **edit**:
/// the intersecting highlight(s) are replaced by a single highlight spanning the
/// **union** of their content and the selection, carrying the new comment
/// (operator: annotating part of an already-annotated span extends it, never
/// nests). `range` is original-source byte offsets.
///
/// The union is computed in *cleaned* (delimiter-free) space — where highlighted
/// text is contiguous — then mapped back to the original region to replace. Any
/// annotation fully enclosed by that region is subsumed into the union (its own
/// comment is dropped); the common case (a selection overlapping ONE highlight) is
/// exact.
///
/// The intersecting annotations' own comments are replaced by `comment` — which is why
/// the caller must show the user what is about to be overwritten first. Ask
/// [`merged_comment_for`] for exactly that text; it shares [`intersecting_highlights`]
/// with this function, so the two cannot disagree about what will be merged.
pub(crate) fn insert_or_extend_highlight(
    source: &str,
    range: Range<usize>,
    comment: &str,
) -> String {
    let ext = super::extract(source);
    let sel_cs = super::original_to_cleaned(&ext.shifts, OriginalByteOffset::new(range.start));
    let sel_ce = super::original_to_cleaned(&ext.shifts, OriginalByteOffset::new(range.end));

    let intersecting = intersecting_highlights(&ext.annotations, sel_cs, sel_ce);
    if intersecting.is_empty() {
        return insert_highlight_comment(source, range, comment);
    }

    // Union of the selection and every intersecting highlight, in CLEANED space.
    // N1-phase-2: `u_cs`/`u_ce` are `CleanedByteOffset`, so they cannot be crossed
    // with the ORIGINAL-space `orig_lo`/`orig_hi` below — the min/max chains only
    // type-check within one space, which is exactly the invariant this arithmetic
    // must hold.
    let u_cs = intersecting
        .iter()
        .map(|a| a.cleaned_content.start)
        .chain(std::iter::once(sel_cs))
        .min()
        .unwrap();
    let u_ce = intersecting
        .iter()
        .map(|a| a.cleaned_content.end)
        .chain(std::iter::once(sel_ce))
        .max()
        .unwrap();
    let union_text = &ext.cleaned[u_cs.raw()..u_ce.raw()];

    // Original region to replace: cover every intersecting construct (delimiters
    // included) AND the selection's plain-text extent beyond them. All in ORIGINAL
    // space — `cleaned_to_original` maps the two cleaned union ends across.
    let orig_lo = intersecting
        .iter()
        .map(|a| a.src_span.start)
        .chain(std::iter::once(super::cleaned_to_original(
            &ext.shifts,
            u_cs,
        )))
        .min()
        .unwrap();
    let orig_hi = intersecting
        .iter()
        .map(|a| a.src_span.end)
        .chain(std::iter::once(super::cleaned_to_original(
            &ext.shifts,
            u_ce,
        )))
        .max()
        .unwrap();

    let mut out = String::with_capacity(source.len() + comment.len() + 12);
    out.push_str(&source[..orig_lo.raw()]);
    out.push_str(HL_OPEN);
    out.push_str(union_text);
    out.push_str(HL_CLOSE);
    out.push_str(COMMENT_OPEN);
    out.push_str(&sanitize_comment(comment));
    out.push_str(COMMENT_CLOSE);
    out.push_str(&source[orig_hi.raw()..]);
    out
}

/// The merge-preview: which existing comments a selection destroys, and how
/// they are shown to the user before it happens.
#[cfg(test)]
mod merged_comment_tests {
    use super::*;

    /// **The confirmation required before the `|` join can be relied on.** The
    /// joined string must survive `sanitize_comment`, round-trip through `{>>…<<}`, and
    /// re-extract as ONE comment with the separator intact — not split into two
    /// annotations, and not mangled.
    ///
    /// It holds because `sanitize_comment` rewrites only `<<}`, and no CriticMarkup
    /// delimiter (`{==` `==}` `{>>` `<<}` `{++` `++}` `{--` `--}` `{~~` `~~}`) contains a
    /// `|`. Asserted rather than reasoned about, because the whole point of the ` | ` choice
    /// was to sidestep this risk — an unverified sidestep is just a guess.
    #[test]
    fn a_joined_comment_round_trips_as_one_comment() {
        let joined = ["note A", "note B"].join(COMMENT_JOIN);
        assert_eq!(joined, "note A | note B");

        let written = insert_highlight_comment("the earth is flat", 13..17, &joined);
        assert_eq!(
            written, "the earth is {==flat==}{>>note A | note B<<}",
            "the separator must survive sanitize_comment untouched"
        );

        let ext = super::super::extract(&written);
        assert_eq!(
            ext.annotations.len(),
            1,
            "the join must re-extract as ONE annotation, not split on the bar"
        );
        assert_eq!(
            ext.annotations[0].comment.as_deref(),
            Some("note A | note B"),
            "and the comment must come back byte-identical"
        );
    }

    /// A comment containing the `<<}` terminator IS sanitised (that is the delimiter, and
    /// escaping it is correct) — pinned so the KKK join is never blamed for it, and so the
    /// one thing `sanitize_comment` does stays deliberate.
    #[test]
    fn only_the_comment_terminator_is_sanitised() {
        assert_eq!(sanitize_comment("a | b"), "a | b");
        assert_eq!(sanitize_comment("bar | pipe"), "bar | pipe");
        assert_eq!(
            sanitize_comment("ends with <<} here"),
            "ends with << } here"
        );
        // Newlines inside a comment are left alone; only the ends are trimmed.
        assert_eq!(sanitize_comment("  two\nlines  "), "two\nlines");
    }

    #[test]
    fn no_overlap_merges_nothing() {
        assert_eq!(merged_comment_for("the earth is flat", 13..17), None);
    }

    #[test]
    fn a_single_overlap_yields_that_comment() {
        // "the earth is {==flat==}{>>citation needed<<}" — re-annotate over "flat".
        let src = "the earth is {==flat==}{>>citation needed<<}";
        assert_eq!(
            merged_comment_for(src, 15..19),
            Some("citation needed".to_string()),
            "the comment about to be overwritten must be offered back to the user"
        );
    }

    /// **The multi-overlap decision (KKK — DECIDED)**: every intersecting comment, joined
    /// with ` | `, in SOURCE order. Nothing is destroyed without the user seeing it first.
    #[test]
    fn multi_overlap_joins_every_comment_in_source_order() {
        let src = "{==alpha==}{>>note A<<} middle {==omega==}{>>note B<<}";
        // A selection spanning from inside "alpha" to inside "omega" intersects both.
        let ext = super::super::extract(src);
        let sel = super::super::cleaned_to_original(&ext.shifts, CleanedByteOffset::new(1)).raw()
            ..super::super::cleaned_to_original(
                &ext.shifts,
                CleanedByteOffset::new(ext.cleaned.len() - 1),
            )
            .raw();
        assert_eq!(
            merged_comment_for(src, sel),
            Some("note A | note B".to_string())
        );
    }

    /// Source order, not document-position-of-selection order and not extraction accident:
    /// a selection built back-to-front must still list the comments in source order.
    #[test]
    fn the_join_is_ordered_by_source_not_by_selection_direction() {
        let src = "{==alpha==}{>>zebra<<} middle {==omega==}{>>apple<<}";
        let ext = super::super::extract(src);
        let sel = super::super::cleaned_to_original(&ext.shifts, CleanedByteOffset::new(1)).raw()
            ..super::super::cleaned_to_original(
                &ext.shifts,
                CleanedByteOffset::new(ext.cleaned.len() - 1),
            )
            .raw();
        assert_eq!(
            merged_comment_for(src, sel),
            Some("zebra | apple".to_string()),
            "source order — NOT alphabetical, and not reversed"
        );
    }

    /// The card and the mutation must agree. Whatever `merged_comment_for` offers, feeding
    /// it straight back to `insert_or_extend_highlight` (the user pressing Save without
    /// editing) must PRESERVE both comments — which is the entire point of KKK: the merge
    /// stops being destructive by default.
    #[test]
    fn committing_the_prepopulated_comment_unchanged_preserves_both_comments() {
        let src = "{==alpha==}{>>note A<<} middle {==omega==}{>>note B<<}";
        let ext = super::super::extract(src);
        let sel = super::super::cleaned_to_original(&ext.shifts, CleanedByteOffset::new(1)).raw()
            ..super::super::cleaned_to_original(
                &ext.shifts,
                CleanedByteOffset::new(ext.cleaned.len() - 1),
            )
            .raw();

        let prepopulated = merged_comment_for(src, sel.clone()).expect("two overlaps");
        let out = insert_or_extend_highlight(src, sel, &prepopulated);

        let after = super::super::extract(&out);
        assert_eq!(after.annotations.len(), 1, "the merge unions into one");
        assert_eq!(
            after.annotations[0].comment.as_deref(),
            Some("note A | note B"),
            "neither reviewer's remark may be lost — BEFORE this fix the new comment simply \
             replaced both, silently"
        );
    }

    /// A bare `{==highlight==}` carries no comment, so it contributes nothing to the join —
    /// and must not produce a stray separator or an empty pre-population.
    #[test]
    fn a_comment_less_highlight_contributes_nothing() {
        let src = "{==bare==} and {==noted==}{>>real note<<}";
        let ext = super::super::extract(src);
        let sel = super::super::cleaned_to_original(&ext.shifts, CleanedByteOffset::new(1)).raw()
            ..super::super::cleaned_to_original(
                &ext.shifts,
                CleanedByteOffset::new(ext.cleaned.len() - 1),
            )
            .raw();
        assert_eq!(
            merged_comment_for(src, sel),
            Some("real note".to_string()),
            "no leading separator from the comment-less highlight"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_comment_wraps_the_range() {
        // "the earth is flat" — annotate "flat" (bytes 13..17).
        let src = "the earth is flat";
        assert_eq!(
            insert_highlight_comment(src, 13..17, "citation needed"),
            "the earth is {==flat==}{>>citation needed<<}"
        );
    }

    #[test]
    fn extend_with_no_overlap_is_a_plain_insert() {
        let src = "the earth is flat";
        assert_eq!(
            insert_or_extend_highlight(src, 13..17, "note"),
            "the earth is {==flat==}{>>note<<}"
        );
    }

    #[test]
    fn extend_inside_an_existing_highlight_re_annotates_the_same_span() {
        // Selecting "uic" (8..11) inside {==quick==} edits it: same span, new comment.
        let src = "the {==quick==}{>>old<<} fox";
        assert_eq!(
            insert_or_extend_highlight(src, 8..11, "new"),
            "the {==quick==}{>>new<<} fox"
        );
    }

    #[test]
    fn extend_past_a_highlight_grows_the_union() {
        // Select "quick" through "brown" (7..30) — the span between crosses the
        // existing construct's delimiters; the result is ONE highlight over the union.
        let src = "the {==quick==}{>>old<<} brown fox";
        assert_eq!(
            insert_or_extend_highlight(src, 7..30, "new"),
            "the {==quick brown==}{>>new<<} fox"
        );
    }

    #[test]
    fn sequential_non_overlapping_creates_both_survive() {
        // TDD §17.14 — "several annotations can be created in one sitting". Two
        // NON-overlapping sequential creates on the same source must both survive as
        // distinct, well-formed constructs. The overlap/extend path is covered by the
        // `extend_*` tests; this pins the "created normally, repeatably" half in the
        // pure logic (the GTK path exercises the same `insert_or_extend_highlight`).
        let src = "alpha beta gamma";
        // First create over "gamma" (11..16).
        let once = insert_or_extend_highlight(src, 11..16, "c-gamma");
        assert_eq!(once, "alpha beta {==gamma==}{>>c-gamma<<}");
        // Second create over "alpha" (0..5): offsets before the first insertion are
        // unchanged and the ranges don't overlap, so this is a fresh insert (the
        // union path would fire only on intersection).
        let twice = insert_or_extend_highlight(&once, 0..5, "c-alpha");
        assert_eq!(
            twice,
            "{==alpha==}{>>c-alpha<<} beta {==gamma==}{>>c-gamma<<}"
        );
        // Both round-trip through the scanner as two distinct, well-formed annotations.
        let ext = crate::annotate::extract(&twice);
        let claims: Vec<&str> = ext
            .annotations
            .iter()
            .map(|a| &ext.cleaned[a.cleaned_content.start.raw()..a.cleaned_content.end.raw()])
            .collect();
        assert_eq!(claims, vec!["alpha", "gamma"]);
        let comments: Vec<&str> = ext
            .annotations
            .iter()
            .filter_map(|a| a.comment.as_deref())
            .collect();
        assert_eq!(comments, vec!["c-alpha", "c-gamma"]);
    }

    #[test]
    fn extend_across_two_highlights_merges_them() {
        // Selecting from "one" through "two" (5..30) merges both existing highlights
        // (and the "and" between) into a single union highlight with the new comment.
        let src = "a {==one==}{>>c1<<} and {==two==}{>>c2<<} b";
        assert_eq!(
            insert_or_extend_highlight(src, 5..30, "merged"),
            "a {==one and two==}{>>merged<<} b"
        );
    }

    #[test]
    fn highlight_comment_preserves_surrounding_text() {
        let src = "before flat after";
        // annotate "flat" (7..11)
        assert_eq!(
            insert_highlight_comment(src, 7..11, "why"),
            "before {==flat==}{>>why<<} after"
        );
    }

    #[test]
    fn highlight_keeps_markup_inside_the_span() {
        // Selecting a span that contains inline markup is fine — CriticMarkup
        // wraps it verbatim (PLAN testing strategy: markup inside a highlight).
        let src = "a **bold** claim";
        assert_eq!(
            insert_highlight_comment(src, 2..10, "rebuttal"),
            "a {==**bold**==}{>>rebuttal<<} claim"
        );
    }

    #[test]
    fn point_comment_inserts_at_offset() {
        let src = "end of thought.";
        // at the very end
        assert_eq!(
            insert_point_comment(src, src.len(), "expand on this"),
            "end of thought.{>>expand on this<<}"
        );
    }

    #[test]
    fn point_comment_anchor_outside_any_construct_is_unchanged() {
        // Plain text, no constructs: the D5 anchor is already safe.
        let src = "First para.\n\nThe earth is flat today.";
        assert_eq!(point_comment_anchor(src, 20), 20);
        // At a construct BOUNDARY (not strictly inside) is also unchanged.
        let annotated = "a {==flat==}{>>cite<<} b";
        let span_start = annotated.find("{==").unwrap(); // 2
        let span_end = annotated.rfind("<<}").unwrap() + 3; // just past the whole construct
        assert_eq!(point_comment_anchor(annotated, span_start), span_start);
        assert_eq!(point_comment_anchor(annotated, span_end), span_end);
    }

    #[test]
    fn point_comment_anchor_snaps_out_of_a_construct_it_lands_inside() {
        // The multi-block-over-existing-annotation defect: the D5 anchor maps into the
        // middle of `{==flat==}{>>cite<<}`. Snap it to the construct's END so the comment
        // attaches after the whole thing rather than splitting it.
        let src = "The earth is {==flat==}{>>cite<<} today.";
        let span_end = src.rfind("<<}").unwrap() + 3;
        let inside = src.find("flat").unwrap() + 1; // between 'f' and 'l', inside {==…==}
        assert_eq!(point_comment_anchor(src, inside), span_end);
        // And splicing at the snapped anchor yields TWO well-formed annotations, the
        // existing one intact — the whole point of the snap.
        let out = insert_point_comment(src, point_comment_anchor(src, inside), "new");
        assert_eq!(out, "The earth is {==flat==}{>>cite<<}{>>new<<} today.");
        let ext = super::super::extract(&out);
        assert_eq!(ext.annotations.len(), 2);
        assert_eq!(ext.annotations[0].comment.as_deref(), Some("cite"));
        assert_eq!(ext.annotations[1].comment.as_deref(), Some("new"));
    }

    #[test]
    fn point_comment_at_a_mid_string_offset() {
        let src = "first. second.";
        // after "first." (offset 6)
        assert_eq!(
            insert_point_comment(src, 6, "note"),
            "first.{>>note<<} second."
        );
    }

    #[test]
    fn comment_cannot_close_its_own_token() {
        // A user typing the closing delimiter inside the comment must not be able
        // to break out of it.
        let src = "claim";
        let out = insert_highlight_comment(src, 0..5, "see <<} here");
        assert_eq!(out, "{==claim==}{>>see << } here<<}");
        // …and the only `<<}` in the output is the real terminator.
        assert_eq!(out.matches(COMMENT_CLOSE).count(), 1);
    }

    #[test]
    fn comment_outer_whitespace_is_trimmed() {
        let src = "x";
        assert_eq!(
            insert_highlight_comment(src, 0..1, "   padded   "),
            "{==x==}{>>padded<<}"
        );
    }

    #[test]
    fn remove_highlight_restores_the_claim() {
        // Round-trip: annotate then remove yields the original source.
        let original = "the earth is flat";
        let annotated = insert_highlight_comment(original, 13..17, "citation needed");
        // In `annotated`, the highlighted "flat" sits just after "{==" (offset 16)
        // for 4 bytes, and the whole construct spans 13..len.
        let inner_start = annotated.find("{==").unwrap() + HL_OPEN.len();
        let inner = inner_start..inner_start + 4;
        let span = 13..annotated.len();
        assert_eq!(
            remove_annotation(&annotated, span, Some(inner)).as_deref(),
            Some(original)
        );
    }

    /// ScrAP-187, floor half. `remove_annotation` is reached with ranges computed
    /// against an EARLIER state of the document, so an out-of-range one is a
    /// reachable state, not a bug to assert on. Raw slicing panicked here — and a
    /// panic inside a GTK signal handler aborts the process, taking unsaved work
    /// with it.
    ///
    /// This is deliberately a direct unit test rather than only an end-to-end one:
    /// the anchored resolution upstream also rejects a vanished construct, so an
    /// end-to-end test passes even with this floor removed. Each defense needs a
    /// guard that fails when THAT defense alone is taken away.
    #[test]
    fn remove_is_total_against_a_range_the_source_cannot_hold() {
        let src = "alpha {==beta==}";
        // Past the end.
        assert_eq!(remove_annotation(src, 6..26, Some(9..13)), None);
        assert_eq!(remove_annotation(src, 100..200, None), None);
        // Kept content outside the construct being removed would splice in text
        // from somewhere else entirely.
        assert_eq!(remove_annotation(src, 6..16, Some(0..5)), None);
        // Not on a char boundary ("Å" is two bytes).
        assert_eq!(remove_annotation("Å{==b==}", 1..8, None), None);
    }

    /// The same floor under Edit — see the note above.
    #[test]
    fn edit_comment_is_total_against_a_range_the_source_cannot_hold() {
        let src = "{>>note<<}";
        assert_eq!(edit_comment(src, 3..99, "x"), None);
        assert_eq!(edit_comment(src, 99..100, "x"), None);
        assert_eq!(edit_comment("Å{>>n<<}", 1..99, "x"), None);
    }

    #[test]
    fn remove_point_comment_deletes_the_whole_construct() {
        let original = "first. second.";
        let annotated = insert_point_comment(original, 6, "note");
        let span = 6..6 + "{>>note<<}".len();
        assert_eq!(
            remove_annotation(&annotated, span, None).as_deref(),
            Some(original)
        );
    }

    #[test]
    fn edit_comment_replaces_only_the_body() {
        let annotated = "{==x==}{>>old note<<}";
        let body_start = annotated.find("{>>").unwrap() + COMMENT_OPEN.len();
        let body_end = annotated.rfind("<<}").unwrap();
        assert_eq!(
            edit_comment(annotated, body_start..body_end, "new note").as_deref(),
            Some("{==x==}{>>new note<<}")
        );
    }

    #[test]
    fn edit_comment_sanitises_the_replacement() {
        let annotated = "{>>old<<}";
        let body = COMMENT_OPEN.len()..COMMENT_OPEN.len() + 3; // "old"
        let out = edit_comment(annotated, body, "bad <<} close").expect("a valid body range");
        assert_eq!(out, "{>>bad << } close<<}");
        assert_eq!(out.matches(COMMENT_CLOSE).count(), 1);
    }

    #[test]
    fn multibyte_span_slices_on_char_boundaries() {
        // "Åπ claim" — annotate "claim" past the 2-byte Å and 2-byte π.
        let src = "Åπ claim";
        let start = "Åπ ".len(); // byte offset of "claim"
        let out = insert_highlight_comment(src, start..src.len(), "unicode-safe");
        assert_eq!(out, "Åπ {==claim==}{>>unicode-safe<<}");
    }

    // ── totality of the INSERT paths (QA round 3, P-6) ────────────────────────

    /// The removal paths were deliberately total and the insertion paths were
    /// not, which is the finding: two functions in one file taking the same kind
    /// of input and disagreeing about whether to trust it. These pin the
    /// insertion side to the same contract.
    ///
    /// The offsets used here are interior bytes of a multi-byte character, which
    /// is the shape a buffer→source mapping error actually produces — an offset
    /// that is in range and still not sliceable. Before the fix each of these
    /// panicked, i.e. aborted the process.
    #[test]
    fn inserting_over_an_unusable_range_returns_the_source_unchanged() {
        // "é" occupies bytes 1..3, so 2 is in range but not a char boundary.
        let source = "aébc";
        assert!(!source.is_char_boundary(2));

        assert_eq!(
            insert_highlight_comment(source, 2..4, "note"),
            source,
            "a non-boundary start must leave the document untouched"
        );
        assert_eq!(
            insert_highlight_comment(source, 0..2, "note"),
            source,
            "a non-boundary end must leave the document untouched"
        );
        assert_eq!(
            insert_point_comment(source, 2, "note"),
            source,
            "a non-boundary anchor must leave the document untouched"
        );
        // Out of range entirely.
        assert_eq!(insert_point_comment(source, 9_999, "note"), source);
        assert_eq!(insert_highlight_comment(source, 0..9_999, "note"), source);
    }

    /// …and the ordinary path is unchanged by the guard, so the degradation
    /// cannot be hiding a regression.
    #[test]
    fn inserting_over_a_valid_range_still_works() {
        assert_eq!(
            insert_highlight_comment("aébc", 0..3, "note"),
            "{==aé==}{>>note<<}bc"
        );
        assert_eq!(insert_point_comment("aébc", 3, "note"), "aé{>>note<<}bc");
    }
}
