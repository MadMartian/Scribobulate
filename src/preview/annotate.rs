//! Part C — the preview-side **create** overlay. When the reader
//! selects text in the preview, a small non-autohide **GtkPopover** offers **Annotate**;
//! clicking it dismisses the popover and reveals an in-surface comment-entry card (an
//! GtkOverlay child) at the same anchor, whose Save wraps the selected source range in
//! CriticMarkup (or, for a cross-block selection, drops a point comment at the selection
//! end) via the view's annotation sink.
//!
//! The selection→source mapping ([`create_from_selection`]) and the entry-card placement
//! geometry ([`bar_placement`], and the shared `saferizer::ViewportRect` anchor guard) are
//! pure functions unit-tested without
//! a display; the GTK wiring around them lives in the [`overlay`] submodule, which is
//! excluded from the coverage ratchet (scripts/coverage.sh / POLICY.md) because it cannot
//! be exercised headlessly — same treatment as `window/editbar/` and `codeview.rs`.

use crate::codeview::CreateAnnotation;
use crate::span::CleanedByteOffset;

mod overlay;
pub(crate) use overlay::{position_card, wire_annotation_overlay};

/// Turn a preview selection `[buf_a, buf_b)` + comment into the annotation to create.
/// The wrap span comes from the render's **copymap** ([`crate::copymap::wrap_span`]),
/// which returns the OUTER, balanced source range — any inline construct the selection
/// touches (emphasis/strong/link/code) is included WHOLE, so the `{==…==}` delimiters can
/// never split a `**`/`` ` ``/`[]()` (the inline-construct-split bug). The cleaned span is
/// translated to original-source bytes via the shift table; a span crossing a block
/// boundary (blank line) falls back to a point comment at the selection END.
/// Returns `None` for an empty selection/comment or an
/// unmappable selection.
pub(crate) fn create_from_selection(
    copymap: &crate::copymap::CopyTree,
    shifts: &[(usize, usize)],
    cleaned: &str,
    buf_a: i32,
    buf_b: i32,
    comment: &str,
) -> Option<CreateAnnotation> {
    let comment = comment.trim().to_string();
    if comment.is_empty() {
        return None;
    }
    Some(
        match selection_target(copymap, shifts, cleaned, buf_a, buf_b)? {
            SelectionTarget::Point(at) => CreateAnnotation::Point { at, comment },
            SelectionTarget::Highlight(range) => CreateAnnotation::Highlight { range, comment },
        },
    )
}

/// Where in the ORIGINAL source a preview selection lands, independent of any comment.
/// See [`selection_target`].
pub(crate) enum SelectionTarget {
    /// An intra-block selection, wrapped as a highlight over this source byte range.
    Highlight(std::ops::Range<usize>),
    /// A selection crossing a block boundary — a point comment at this source byte
    /// offset instead (the cross-block fallback).
    Point(usize),
}

/// Resolve a preview buffer selection to its target in the ORIGINAL source.
///
/// The shared first half of [`create_from_selection`], split out because the
/// comment card must pre-populate from the annotations the commit is *going* to merge, so
/// both must resolve the selection to the same source range. Re-deriving the mapping at the
/// card would be a second implementation free to drift, and any drift shows up as the card
/// displaying comments that are not the ones destroyed — worse than no pre-population.
///
/// Returns `None` for an empty/degenerate selection.
pub(crate) fn selection_target(
    copymap: &crate::copymap::CopyTree,
    shifts: &[(usize, usize)],
    cleaned: &str,
    buf_a: i32,
    buf_b: i32,
) -> Option<SelectionTarget> {
    let (lo, hi) = if buf_a <= buf_b {
        (buf_a, buf_b)
    } else {
        (buf_b, buf_a)
    };
    let span = crate::copymap::wrap_span(copymap, cleaned, lo, hi)?;
    if span.start >= span.end {
        return None;
    }
    // `span` is a range in CLEANED bytes (from `wrap_span` over `cleaned`); the
    // SelectionTarget holds ORIGINAL bytes — the N1-typed cleaned→original crossing.
    // `.get()`, not a raw slice (QA round 3, P-3). `span` comes back from
    // `wrap_span` over a copymap built from a DIFFERENT (cleaned) string than the
    // one indexed here, and its ends are byte arithmetic — nothing has proved
    // they land on char boundaries. A raw slice panics on that, and a panic on a
    // selection handler is a process abort. Treating an unusable span as
    // "does not cross a block" is the conservative answer: it produces a
    // Highlight rather than a Point, which the caller can still act on.
    if cleaned
        .get(span.clone())
        .is_some_and(|s| s.contains("\n\n"))
    {
        Some(SelectionTarget::Point(
            crate::annotate::cleaned_to_original(shifts, CleanedByteOffset::new(span.end)).raw(),
        ))
    } else {
        Some(SelectionTarget::Highlight(
            crate::annotate::cleaned_to_original(shifts, CleanedByteOffset::new(span.start)).raw()
                ..crate::annotate::cleaned_to_original(shifts, CleanedByteOffset::new(span.end))
                    .raw(),
        ))
    }
}

/// Turn an EDITOR selection into the annotation to create. The editor buffer holds
/// the RAW Markdown source verbatim, so — unlike the preview's [`create_from_selection`]
/// — there is NO copymap/shift indirection: the selection's character offsets convert
/// directly to source BYTE offsets (mind the char→byte gap on non-ASCII text). The raw
/// span is then balanced by [`crate::copymap::balance_source_span`] — the editor-side
/// sibling of the preview's `wrap_span` — so an annotation can never split an inline
/// construct. A selection crossing a block boundary (a blank line)
/// falls back to a point comment at the selection END; an
/// intra-block selection wraps as a highlight.
/// Returns `None` for an empty/degenerate selection or an empty comment.
pub(crate) fn create_from_editor_selection(
    source: &str,
    char_a: i32,
    char_b: i32,
    comment: &str,
) -> Option<CreateAnnotation> {
    let comment = comment.trim().to_string();
    if comment.is_empty() {
        return None;
    }
    Some(match editor_selection_target(source, char_a, char_b)? {
        SelectionTarget::Point(at) => CreateAnnotation::Point { at, comment },
        SelectionTarget::Highlight(range) => CreateAnnotation::Highlight { range, comment },
    })
}

/// Resolve an EDITOR selection to its target in the original source, independent of any
/// comment — the editor-side sibling of [`selection_target`].
///
/// The shared first half of [`create_from_editor_selection`], split out for the same reason
/// the preview's was (b4b9d6a): the comment card must pre-populate from the annotations the
/// commit is *going* to merge, so both must resolve the selection to the same source range.
/// Re-deriving the char→byte + balance mapping at the card would be a second implementation
/// free to drift, and any drift shows up as the card displaying comments that are not the
/// ones destroyed — worse than no pre-population at all.
///
/// Returns `None` for an empty/degenerate selection.
pub(crate) fn editor_selection_target(
    source: &str,
    char_a: i32,
    char_b: i32,
) -> Option<SelectionTarget> {
    let (lo, hi) = if char_a <= char_b {
        (char_a, char_b)
    } else {
        (char_b, char_a)
    };
    if lo < 0 || hi <= lo {
        return None;
    }
    let start = char_to_byte(source, lo as usize)?;
    let end = char_to_byte(source, hi as usize)?;
    if start >= end {
        return None;
    }
    // Balance BEFORE the block-crossing test: swallowing a construct can only widen
    // the span, and the widened span is what actually gets wrapped — so it is the one
    // that must be tested for the cross-block point-comment fallback. The caller
    // normalises (ScrAP-75, `NormalizedMd`'s doc): the substitution is length- and
    // position-preserving, so the returned range still indexes `source` unchanged.
    let normalized = crate::renderer::NormalizedMd::new(source);
    let span = crate::copymap::balance_source_span(&normalized, start..end);
    // Same reasoning as the cleaned-side sibling above (QA round 3, P-3):
    // `balance_source_span` returns byte arithmetic over the source, not a
    // proven char boundary.
    if source.get(span.clone()).is_some_and(|s| s.contains("\n\n")) {
        Some(SelectionTarget::Point(span.end))
    } else {
        Some(SelectionTarget::Highlight(span))
    }
}

/// Byte offset of the `char_off`-th character boundary in `s` (its length when
/// `char_off` equals the character count), or `None` when `char_off` is past the end.
fn char_to_byte(s: &str, char_off: usize) -> Option<usize> {
    s.char_indices()
        .map(|(b, _)| b)
        .chain(std::iter::once(s.len()))
        .nth(char_off)
}

/// Gap in px between the Annotate entry card and the selection line it anchors to.
const BAR_GAP: i32 = 2;

/// Pure placement math for the Annotate entry card (unit-tested without a display). Given
/// the popover anchor in overlay-local coords (`ax` = selection-midpoint x, `ay` =
/// selection first-line top), its line height, the card's natural size (`bw`×`bh`), and
/// the overlay's visible size (`ow`×`oh`), return the clamped top-left
/// `(margin_start, margin_top)`. The card is CENTERED horizontally on `ax` and placed
/// ABOVE `ay` — matching where the action popover's body sat, so the entry appears in the
/// popover's place rather than jumping to a corner — falling back to BELOW when there is
/// no room above, and clamped on both axes so it never sits half-off an edge (with
/// `clip-overlay` set, GTK would otherwise clip it away).
pub(crate) fn bar_placement(
    ax: i32,
    ay: i32,
    line_h: i32,
    bw: i32,
    bh: i32,
    ow: i32,
    oh: i32,
) -> (i32, i32) {
    let x = (ax - bw / 2).clamp(0, (ow - bw).max(0)); // centered on the anchor
    let mut y = ay - bh - BAR_GAP; // above the selection's first line
    if y < 0 {
        y = ay + line_h + BAR_GAP; // below it if there's no room above
    }
    y = y.clamp(0, (oh - bh).max(0));
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codeview::CreateAnnotation;
    use crate::copymap::{build, Construct, CopyTree, RawEv, RawKind};

    /// A copymap for a single plain-text paragraph (no inline constructs).
    fn plain_tree(md: &str) -> CopyTree {
        let n = md.chars().count() as i32;
        let evs = vec![
            RawEv {
                buf: (0, 0),
                src: 0..md.len(),
                kind: RawKind::Start(Construct::Paragraph),
            },
            RawEv {
                buf: (0, n),
                src: 0..md.len(),
                kind: RawKind::Text(md.to_string()),
            },
            RawEv {
                buf: (n, n),
                src: 0..md.len(),
                kind: RawKind::End(Construct::Paragraph),
            },
        ];
        build(md, &evs, n)
    }

    /// A copymap for two plain paragraphs "para one\n\npara two" (the `\n\n` separator
    /// sits in the inter-paragraph buffer gap, buf 8→10).
    fn two_para_tree() -> (CopyTree, String) {
        let md = "para one\n\npara two".to_string();
        let evs = vec![
            RawEv {
                buf: (0, 0),
                src: 0..8,
                kind: RawKind::Start(Construct::Paragraph),
            },
            RawEv {
                buf: (0, 8),
                src: 0..8,
                kind: RawKind::Text("para one".into()),
            },
            RawEv {
                buf: (8, 8),
                src: 0..8,
                kind: RawKind::End(Construct::Paragraph),
            },
            RawEv {
                buf: (8, 10),
                src: 10..18,
                kind: RawKind::Start(Construct::Paragraph),
            },
            RawEv {
                buf: (10, 18),
                src: 10..18,
                kind: RawKind::Text("para two".into()),
            },
            RawEv {
                buf: (18, 18),
                src: 10..18,
                kind: RawKind::End(Construct::Paragraph),
            },
        ];
        (build(&md, &evs, 18), md)
    }

    #[test]
    fn selection_within_a_block_becomes_a_highlight() {
        let cleaned = "the earth is flat";
        let tree = plain_tree(cleaned);
        match create_from_selection(&tree, &[(0, 0)], cleaned, 13, 17, "citation needed") {
            Some(CreateAnnotation::Highlight { range, comment }) => {
                assert_eq!(range, 13..17);
                assert_eq!(&cleaned[range], "flat");
                assert_eq!(comment, "citation needed");
            }
            other => panic!("expected a highlight, got {other:?}"),
        }
    }

    #[test]
    fn empty_comment_is_rejected() {
        let cleaned = "the earth is flat";
        let tree = plain_tree(cleaned);
        assert!(create_from_selection(&tree, &[(0, 0)], cleaned, 13, 17, "   ").is_none());
    }

    #[test]
    fn highlight_range_translates_through_a_nonidentity_shift_table() {
        // cleaned "x flat" from an original where the kept text starts +3 (a "{=="
        // removed at cleaned 2): cleaned offset >= 2 maps to original + 3.
        let cleaned = "x flat";
        let tree = plain_tree(cleaned);
        let shifts = [(0usize, 0usize), (2usize, 5usize)];
        match create_from_selection(&tree, &shifts, cleaned, 2, 6, "why") {
            Some(CreateAnnotation::Highlight { range, .. }) => assert_eq!(range, 5..9),
            other => panic!("expected a highlight, got {other:?}"),
        }
    }

    #[test]
    fn entry_card_is_centered_above_the_anchor_when_there_is_room() {
        // Anchor (midpoint x = 400) mid-viewport: the card centers on it (ax - bw/2) and
        // sits its height + gap ABOVE the anchor y — matching the popover's body.
        let (x, y) = bar_placement(400, 200, 18, 340, 40, 800, 600);
        assert_eq!(x, 400 - 340 / 2, "centered horizontally on the anchor");
        assert_eq!(y, 200 - 40 - BAR_GAP, "placed above the selection line");
    }

    #[test]
    fn entry_card_falls_back_below_when_there_is_no_room_above() {
        // Anchor near the very top: no room for the card above, so it goes BELOW the
        // selection's first line (anchor y + line_h + gap).
        let (_x, y) = bar_placement(400, 10, 18, 340, 40, 800, 600);
        assert_eq!(y, 10 + 18 + BAR_GAP);
    }

    #[test]
    fn entry_card_is_clamped_inside_the_overlay_on_both_axes() {
        // Anchor near the right edge: the centered x clamps to overlay_width - bar_width.
        let (x, _y) = bar_placement(900, 300, 18, 340, 40, 800, 600);
        assert_eq!(x, 800 - 340);
        // A card taller than the overlay (degenerate) clamps y to 0, never negative.
        let (_x2, y2) = bar_placement(400, 300, 18, 340, 700, 800, 600);
        assert_eq!(y2, 0);
        // Overlay narrower than the card (degenerate) clamps x to 0, never negative.
        let (x3, _y3) = bar_placement(400, 300, 18, 900, 40, 800, 600);
        assert_eq!(x3, 0);
    }

    #[test]
    fn editor_selection_within_a_block_becomes_a_byte_range_highlight() {
        let src = "the earth is flat here";
        match create_from_editor_selection(src, 13, 17, "citation needed") {
            Some(CreateAnnotation::Highlight { range, comment }) => {
                assert_eq!(range, 13..17);
                assert_eq!(&src[range], "flat");
                assert_eq!(comment, "citation needed");
            }
            other => panic!("expected a highlight, got {other:?}"),
        }
    }

    #[test]
    fn editor_selection_char_offsets_convert_to_byte_offsets_on_multibyte_text() {
        // "café " is 5 chars but 6 bytes (é = 2 bytes); a selection of "flat" (chars
        // 5..9) must map to BYTES 6..10, not 5..9.
        let src = "café flat";
        match create_from_editor_selection(src, 5, 9, "why") {
            Some(CreateAnnotation::Highlight { range, .. }) => {
                assert_eq!(range, 6..10);
                assert_eq!(&src[range], "flat");
            }
            other => panic!("expected a highlight, got {other:?}"),
        }
    }

    #[test]
    fn editor_selection_crossing_a_blank_line_becomes_a_point_comment_at_the_end() {
        let src = "para one\n\npara two";
        // chars 5..15 span the blank-line separator.
        match create_from_editor_selection(src, 5, 15, "spans blocks") {
            Some(CreateAnnotation::Point { at, comment }) => {
                assert_eq!(at, 15, "point comment anchors at the selection end (byte)");
                assert_eq!(comment, "spans blocks");
            }
            other => panic!("expected a point comment, got {other:?}"),
        }
    }

    /// The editor card must offer back the comment its commit is about to overwrite.
    ///
    /// The editor-side equivalent of the preview's pre-population (b4b9d6a). The card
    /// resolves the selection with `editor_selection_target` and asks `merged_comment_for`
    /// — the same pair the commit uses — so what is shown is exactly what is destroyed.
    #[test]
    fn editor_card_offers_back_the_comment_an_overlap_would_destroy() {
        let src = "the earth is {==flat==}{>>citation needed<<}";
        // Select "flat" in the RAW editor buffer: it sits at chars 16..20 (inside `{==`).
        let Some(SelectionTarget::Highlight(range)) = editor_selection_target(src, 16, 20) else {
            panic!("an intra-block editor selection must resolve to a highlight");
        };
        assert_eq!(
            crate::annotate::merged_comment_for(src, range),
            Some("citation needed".to_string()),
            "the editor card must pre-populate with the comment about to be overwritten"
        );
    }

    /// **The data loss, disproved directly.** Committing the pre-populated comment
    /// UNCHANGED must preserve BOTH existing comments. Before the pre-population the card
    /// handed the mutation only the newly typed text, so `insert_or_extend_highlight`
    /// replaced both intersecting constructs — comments included — with the union carrying
    /// that text alone, and the reviewer's earlier remarks were gone with no prompt and no
    /// trace. This is the whole defect in one assertion.
    #[test]
    fn editor_committing_the_prepopulated_comment_unchanged_preserves_both_comments() {
        let src = "{==alpha==}{>>note A<<} middle {==omega==}{>>note B<<}";
        // A selection spanning from inside "alpha" through to inside "omega" — in RAW
        // editor chars, from just after `{==` to just before the trailing `<<}`.
        let a = 4;
        let b = (src.chars().count() - 4) as i32;
        let Some(SelectionTarget::Highlight(range)) = editor_selection_target(src, a, b) else {
            panic!("expected a highlight over the span covering both annotations");
        };

        // What the card shows the user…
        let shown = crate::annotate::merged_comment_for(src, range).expect("both comments merge");
        assert_eq!(shown, "note A | note B");

        // …is committed back verbatim, exactly as a user who typed nothing would.
        let create = create_from_editor_selection(src, a, b, &shown)
            .expect("the pre-populated comment must commit");
        let CreateAnnotation::Highlight { range, comment } = create else {
            panic!("expected a highlight create");
        };
        let out = crate::annotate::insert_or_extend_highlight(src, range, &comment);

        assert!(
            out.contains("note A") && out.contains("note B"),
            "committing the pre-populated comment unchanged must preserve BOTH comments; got: {out}"
        );
        assert_eq!(
            crate::annotate::merged_comment_for(&out, 0..out.len()),
            Some("note A | note B".to_string()),
            "the surviving comments must round-trip as one merged comment"
        );
    }

    /// A point comment inserts a NEW construct rather than replacing any, so it merges
    /// nothing and must pre-populate nothing — an empty card, not a borrowed comment.
    #[test]
    fn editor_cross_block_point_comment_merges_nothing() {
        let src = "{==alpha==}{>>note A<<}\n\npara two";
        let target = editor_selection_target(src, 4, (src.chars().count() - 1) as i32);
        assert!(
            matches!(target, Some(SelectionTarget::Point(_))),
            "a selection crossing a blank line is a point comment"
        );
    }

    /// The card and the commit must resolve a selection to the SAME source range. They
    /// share `editor_selection_target` precisely so they cannot drift; this pins the
    /// sharing so a future refactor that re-derives the mapping at one of them fails here
    /// rather than silently showing comments that are not the ones destroyed.
    #[test]
    fn editor_card_and_commit_resolve_the_same_range() {
        for (src, a, b) in [
            ("the earth is {==flat==}{>>note<<}", 16, 20),
            ("plain text here", 6, 10),
            ("a **bold** claim", 4, 8),
            ("café {==flat==}{>>note<<}", 9, 13),
        ] {
            let via_target = match editor_selection_target(src, a, b) {
                Some(SelectionTarget::Highlight(r)) => Some(r),
                _ => None,
            };
            let via_commit = match create_from_editor_selection(src, a, b, "x") {
                Some(CreateAnnotation::Highlight { range, .. }) => Some(range),
                _ => None,
            };
            assert_eq!(
                via_target, via_commit,
                "card and commit must agree on the source range for {src:?} [{a}..{b}]"
            );
        }
    }

    #[test]
    fn editor_selection_rejects_empty_comment_and_degenerate_range() {
        let src = "the earth is flat";
        assert!(create_from_editor_selection(src, 13, 17, "   ").is_none());
        assert!(create_from_editor_selection(src, 13, 13, "x").is_none());
        // Reversed offsets are normalised, not rejected.
        assert!(create_from_editor_selection(src, 17, 13, "x").is_some());
    }

    // ── an editor annotation must not split an inline construct ──

    #[test]
    fn ww_editor_selection_inside_strong_wraps_the_whole_construct() {
        // The exact case: selecting "bol" inside `**bold**` used to
        // produce `**{==bol==}{>>note<<}d**`, splitting the emphasis run.
        let src = "a **bold** b";
        // chars 4..7 = "bol", strictly inside the `**` delimiters.
        match create_from_editor_selection(src, 4, 7, "note") {
            Some(CreateAnnotation::Highlight { range, .. }) => {
                assert_eq!(&src[range.clone()], "**bold**");
                assert_eq!(range, 2..10);
            }
            other => panic!("expected a balanced highlight, got {other:?}"),
        }
    }

    #[test]
    fn ww_editor_selection_of_plain_text_stays_char_precise() {
        // The balancer must not over-reach: prose touching no construct is untouched.
        let src = "the earth is flat here";
        match create_from_editor_selection(src, 13, 17, "cite") {
            Some(CreateAnnotation::Highlight { range, .. }) => {
                assert_eq!(range, 13..17);
                assert_eq!(&src[range], "flat");
            }
            other => panic!("expected a highlight, got {other:?}"),
        }
    }

    #[test]
    fn ww_editor_selection_exactly_covering_a_construct_is_not_widened() {
        let src = "a **bold** b";
        // chars 2..10 = exactly `**bold**` — already balanced, must not grow.
        match create_from_editor_selection(src, 2, 10, "note") {
            Some(CreateAnnotation::Highlight { range, .. }) => assert_eq!(range, 2..10),
            other => panic!("expected a highlight, got {other:?}"),
        }
    }

    #[test]
    fn ww_editor_selection_straddling_a_code_span_swallows_it_whole() {
        // A code span is atomic — splitting its backticks breaks the construct.
        let src = "run `cargo test` now";
        // chars 5..11 = "cargo " — starts inside the code span's content.
        match create_from_editor_selection(src, 5, 11, "which?") {
            Some(CreateAnnotation::Highlight { range, .. }) => {
                assert!(
                    src[range.clone()].starts_with('`') && src[range.clone()].ends_with('`'),
                    "code span must be swallowed whole, got {:?}",
                    &src[range]
                );
            }
            other => panic!("expected a highlight, got {other:?}"),
        }
    }

    #[test]
    fn ww_editor_selection_straddling_a_link_swallows_the_whole_target() {
        // Splitting `[]()` would leave a dangling destination.
        let src = "see [the docs](https://example.com) ok";
        // chars 6..12 = "he doc" — inside the link TEXT only.
        match create_from_editor_selection(src, 6, 12, "stale?") {
            Some(CreateAnnotation::Highlight { range, .. }) => {
                assert_eq!(&src[range], "[the docs](https://example.com)");
            }
            other => panic!("expected a highlight, got {other:?}"),
        }
    }

    #[test]
    fn ww_editor_balancing_reaches_a_fixpoint_through_nesting() {
        // Swallowing the inner link must then straddle — and swallow — the outer
        // strong, which a single non-iterating pass would miss.
        let src = "x **a [link](u) b** y";
        // chars 8..11 = "ink" — deep inside the nested link.
        match create_from_editor_selection(src, 8, 11, "note") {
            Some(CreateAnnotation::Highlight { range, .. }) => {
                assert_eq!(&src[range], "**a [link](u) b**");
            }
            other => panic!("expected a highlight, got {other:?}"),
        }
    }

    #[test]
    fn ww_editor_selection_inside_an_in_crate_construct_wraps_the_whole_construct() {
        // The four constructs pulldown-cmark does NOT parse here — `==highlight==`,
        // `~~strike~~`, `^sup^`, `~sub~` — are tokenised by `renderer::scan_script_spans`
        // and reach the balancer as plain `Text`, so a pulldown-only walk saw nothing to
        // balance and spliced `{==…==}` BETWEEN the delimiters (ScrAP-195). Each must be
        // swallowed whole, exactly like `**bold**` above.
        for (src, sub, want) in [
            ("a ==mark== b", "ar", "==mark=="),
            ("a ~~strike~~ b", "rik", "~~strike~~"),
            ("a ^sup^ b", "u", "^sup^"),
            ("a ~sub~ b", "u", "~sub~"),
        ] {
            let s = src.find(sub).unwrap();
            match create_from_editor_selection(src, s as i32, (s + sub.len()) as i32, "note") {
                Some(CreateAnnotation::Highlight { range, .. }) => {
                    assert_eq!(&src[range], want, "balanced span for {src:?}")
                }
                other => panic!("expected a balanced highlight for {src:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn ww_editor_selection_straddling_both_tokenisers_balances_both() {
        // One selection running from inside an in-crate construct into a
        // pulldown-owned one: BOTH must be swallowed, so the two passes have to
        // contribute to the same union. (A construct that merely *contains* the
        // selection is already balanced by the pulldown pass alone — this is the
        // shape that isolates the scanner pass.)
        let src = "a ==mark== and **bold** b";
        let from = src.find("ark").unwrap(); // inside the highlight content
        let to = src.find("ld*").unwrap(); // inside the strong content
        match create_from_editor_selection(src, from as i32, to as i32, "note") {
            Some(CreateAnnotation::Highlight { range, .. }) => {
                assert_eq!(&src[range], "==mark== and **bold**")
            }
            other => panic!("expected a balanced highlight, got {other:?}"),
        }
    }

    #[test]
    fn ww_editor_balancing_does_not_widen_over_literal_markers() {
        // The scanner's tight-flanking rules must gate the balancer too: prose where
        // `==`/`^`/`~` are operators or spaced is NOT a construct, so a selection
        // touching one stays char-precise (over-reach would swallow unrelated prose).
        for (src, sub) in [("a == b", "="), ("2^10 ok", "^1"), ("a ~ b", "~")] {
            let s = src.find(sub).unwrap();
            match create_from_editor_selection(src, s as i32, (s + sub.len()) as i32, "note") {
                Some(CreateAnnotation::Highlight { range, .. }) => {
                    assert_eq!(range, s..s + sub.len(), "must not widen in {src:?}")
                }
                other => panic!("expected a highlight for {src:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn ww_balanced_span_that_grows_across_a_blank_line_becomes_a_point_comment() {
        // The block-crossing test must run on the BALANCED span, not the raw one:
        // if balancing widened the span across a blank line, wrapping is invalid.
        let src = "para one\n\npara two";
        match create_from_editor_selection(src, 5, 15, "spans blocks") {
            Some(CreateAnnotation::Point { at, .. }) => assert_eq!(at, 15),
            other => panic!("expected a point comment, got {other:?}"),
        }
    }

    /// A two-paragraph copymap over CLEANED text whose second paragraph carries an
    /// existing `{==flat==}{>>cite<<}` (stripped to bare "flat" in cleaned, as the real
    /// render pipeline does). Returns `(copymap, shifts, cleaned, original)`; buffer
    /// offsets are identity with cleaned offsets for plain paragraphs (the `\n\n`
    /// separator is two chars in both), matching `two_para_tree`.
    fn two_para_with_highlight() -> (CopyTree, Vec<(usize, usize)>, String, String) {
        let original = "First para.\n\nThe earth is {==flat==}{>>cite<<} today.".to_string();
        let ext = crate::annotate::extract(&original);
        let cleaned = ext.cleaned.clone();
        let p1_end = cleaned.find("\n\n").unwrap();
        let p2_start = p1_end + 2;
        let n = cleaned.chars().count() as i32;
        let evs = vec![
            RawEv {
                buf: (0, 0),
                src: 0..p1_end,
                kind: RawKind::Start(Construct::Paragraph),
            },
            RawEv {
                buf: (0, p1_end as i32),
                src: 0..p1_end,
                kind: RawKind::Text(cleaned[..p1_end].to_string()),
            },
            RawEv {
                buf: (p1_end as i32, p1_end as i32),
                src: 0..p1_end,
                kind: RawKind::End(Construct::Paragraph),
            },
            RawEv {
                buf: (p1_end as i32, p2_start as i32),
                src: p2_start..cleaned.len(),
                kind: RawKind::Start(Construct::Paragraph),
            },
            RawEv {
                buf: (p2_start as i32, n),
                src: p2_start..cleaned.len(),
                kind: RawKind::Text(cleaned[p2_start..].to_string()),
            },
            RawEv {
                buf: (n, n),
                src: p2_start..cleaned.len(),
                kind: RawKind::End(Construct::Paragraph),
            },
        ];
        (build(&cleaned, &evs, n), ext.shifts, cleaned, original)
    }

    /// **The multi-block-over-existing-annotation defect, proven at the pure boundary.** A
    /// multi-block preview selection whose END lands part-way through an existing
    /// highlight resolves to a cross-block `Point` whose original anchor maps INSIDE the
    /// `{==flat==}` construct. Committing that anchor UNGUARDED splices `{>>comment<<}`
    /// into the middle of the construct (`{==fl{>>comment<<}at==}`), which re-extracts as
    /// the comment being swallowed into the claim text — no new annotation, source
    /// corrupted. The `point_comment_anchor` snap (the choke-point fix in
    /// `apply_annotation_edit`) must land the comment cleanly AFTER the whole construct.
    #[test]
    fn multiblock_over_existing_annotation_point_lands_outside_the_construct() {
        let (tree, shifts, cleaned, original) = two_para_with_highlight();
        // Select from inside para 1 through "fl" of "flat" in para 2 (ends inside the
        // existing highlight).
        let a = 5;
        let b = cleaned.find("flat").unwrap() as i32 + 2;
        let Some(SelectionTarget::Point(at)) = selection_target(&tree, &shifts, &cleaned, a, b)
        else {
            panic!("a cross-block selection must resolve to a Point");
        };
        // UNGUARDED, the anchor lands strictly inside the existing construct — this is
        // the corruption mechanism (candidate (b)), pinned so a regression is caught.
        let hl = &crate::annotate::extract(&original).annotations[0];
        assert!(
            hl.src_span.start.raw() < at && at < hl.src_span.end.raw(),
            "precondition: the raw D5 anchor {at} lands inside the construct {:?}",
            hl.src_span
        );
        // The fix: snap outside, then splice.
        let safe = crate::annotate::point_comment_anchor(&original, at);
        assert_eq!(
            safe,
            hl.src_span.end.raw(),
            "the anchor snaps to the construct end"
        );
        let out = crate::annotate::insert_point_comment(&original, safe, "my comment");

        // Both the existing highlight+comment AND the new point comment survive, as two
        // distinct, well-formed annotations — nothing swallowed, nothing corrupted.
        let re = crate::annotate::extract(&out);
        assert_eq!(
            re.annotations.len(),
            2,
            "existing highlight + new point comment; got {out:?}"
        );
        assert_eq!(
            &re.cleaned[re.annotations[0].cleaned_content.start.raw()
                ..re.annotations[0].cleaned_content.end.raw()],
            "flat"
        );
        assert_eq!(re.annotations[0].comment.as_deref(), Some("cite"));
        assert_eq!(re.annotations[1].kind, crate::annotate::AnnKind::Comment);
        assert_eq!(re.annotations[1].comment.as_deref(), Some("my comment"));
    }

    #[test]
    fn cross_block_selection_becomes_a_point_comment_at_the_end() {
        let (tree, cleaned) = two_para_tree();
        match create_from_selection(&tree, &[(0, 0)], &cleaned, 5, 15, "spans blocks") {
            Some(CreateAnnotation::Point { at, comment }) => {
                assert_eq!(at, 15, "point comment anchors at the selection end");
                assert_eq!(comment, "spans blocks");
            }
            other => panic!("expected a point comment, got {other:?}"),
        }
    }

    // ── table-cell annotation validation: cell copymap + create_from_selection composition ──
    //
    // Mirrors `preview/build.rs` cell capture: buf is 0-based cell-local, src is
    // cleaned-document-absolute. NO mocks — real `copymap::{build,classify,cell_width}`
    // and real `create_from_selection` / `annotate::extract`.

    use pulldown_cmark::{Event, Parser, Tag, TagEnd};

    /// Per-cell `(CopyTree, content events for display mapping, plain label text)`.
    /// Content events are `(src_start, src_end, buf_before, buf_after)` — Text/Code/Break
    /// only, same filter as body `hl_evs`.
    #[allow(clippy::type_complexity)]
    fn cell_products(cleaned: &str) -> Vec<(CopyTree, Vec<(usize, usize, i32, i32)>, String)> {
        let mut out = Vec::new();
        let mut active = false;
        let mut evs: Vec<RawEv> = Vec::new();
        let mut content: Vec<(usize, usize, i32, i32)> = Vec::new();
        let mut plain = String::new();
        let mut off = 0i32;
        for (ev, src) in Parser::new_ext(cleaned, crate::renderer::md_options()).into_offset_iter()
        {
            let kind = crate::copymap::classify(&ev);
            match &ev {
                Event::Start(Tag::TableCell) => {
                    active = true;
                    evs.clear();
                    content.clear();
                    plain.clear();
                    off = 0;
                }
                Event::End(TagEnd::TableCell) => {
                    out.push((build(cleaned, &evs, off), content.clone(), plain.clone()));
                    active = false;
                }
                _ if active => {
                    if let Some(k) = &kind {
                        let w = crate::copymap::cell_width(k);
                        let before = off;
                        let after = off + w;
                        evs.push(RawEv {
                            buf: (before, after),
                            src: src.clone(),
                            kind: k.clone(),
                        });
                        let is_content =
                            matches!(k, RawKind::Text(_) | RawKind::Code(_) | RawKind::Break);
                        if is_content && after > before {
                            content.push((src.start, src.end, before, after));
                            match k {
                                RawKind::Text(t) | RawKind::Code(t) => plain.push_str(t),
                                RawKind::Break => plain.push('\n'),
                                _ => {}
                            }
                        }
                        off = after;
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// Char offset of `needle` in a cell's plain label text.
    fn plain_off(plain: &str, needle: &str) -> i32 {
        let byte = plain
            .find(needle)
            .unwrap_or_else(|| panic!("needle {needle:?} in {plain:?}"));
        plain[..byte].chars().count() as i32
    }

    #[test]
    fn xx_cell_selection_maps_to_original_via_create_from_selection() {
        // Identity shifts (no prior CriticMarkup): cell-local "flat" → original "flat".
        let original = "| the earth is flat here | x |\n| --- | --- |\n| y | z |\n";
        let ext = crate::annotate::extract(original);
        let cells = cell_products(&ext.cleaned);
        // Header row cell 0: plain "the earth is flat here"
        let (tree, _evs, plain) = &cells[0];
        assert_eq!(plain, "the earth is flat here");
        let a = plain_off(plain, "flat");
        let b = a + "flat".chars().count() as i32;
        match create_from_selection(tree, &ext.shifts, &ext.cleaned, a, b, "citation needed") {
            Some(CreateAnnotation::Highlight { range, comment }) => {
                assert_eq!(&original[range.clone()], "flat");
                assert_eq!(comment, "citation needed");
            }
            other => panic!("expected highlight, got {other:?}"),
        }
    }

    #[test]
    fn xx_cell_selection_translates_through_real_shift_table() {
        // CriticMarkup BEFORE the table shifts cleaned→original; the cell copymap is
        // built against cleaned, so create_from_selection must compose both maps.
        let original =
            "{==prior==}{>>p<<}\n\n| the earth is flat here | x |\n| --- | --- |\n| y | z |\n";
        let ext = crate::annotate::extract(original);
        assert_ne!(
            ext.shifts,
            vec![(0, 0)],
            "fixture must produce a non-identity shift table"
        );
        let cells = cell_products(&ext.cleaned);
        let (tree, _evs, plain) = &cells[0];
        let a = plain_off(plain, "flat");
        let b = a + "flat".chars().count() as i32;
        match create_from_selection(tree, &ext.shifts, &ext.cleaned, a, b, "cite") {
            Some(CreateAnnotation::Highlight { range, comment }) => {
                assert_eq!(
                    &original[range.clone()],
                    "flat",
                    "original slice must be the claim text, not CriticMarkup or shifted junk"
                );
                assert_eq!(comment, "cite");
                // And the range must NOT be the identity cleaned offsets (composition
                // actually moved through shifts).
                let cleaned_flat = ext.cleaned.find("flat").unwrap();
                assert_ne!(
                    range.start, cleaned_flat,
                    "with a prior annotation, original offsets must differ from cleaned"
                );
            }
            other => panic!("expected highlight, got {other:?}"),
        }
    }

    #[test]
    fn xx_cell_selection_over_bold_wraps_the_whole_construct() {
        // wrap_span must still balance **…** when the selection is cell-local.
        let original = "| see **flat** here | x |\n| --- | --- |\n| y | z |\n";
        let ext = crate::annotate::extract(original);
        let cells = cell_products(&ext.cleaned);
        let (tree, _evs, plain) = &cells[0];
        assert_eq!(plain, "see flat here");
        let a = plain_off(plain, "flat");
        let b = a + "flat".chars().count() as i32;
        match create_from_selection(tree, &ext.shifts, &ext.cleaned, a, b, "whole") {
            Some(CreateAnnotation::Highlight { range, .. }) => {
                assert_eq!(&original[range], "**flat**");
            }
            other => panic!("expected highlight, got {other:?}"),
        }
    }

    #[test]
    fn xx_display_maps_document_annotation_to_cell_local_markup() {
        // Round-trip display half: extract an annotation already inside a cell,
        // map cleaned_content → cell-local chars via content events, wrap with
        // the real annotate_markup helper.
        let original =
            "| the earth is {==flat==}{>>citation needed<<} here | x |\n| --- | --- |\n| y | z |\n";
        let ext = crate::annotate::extract(original);
        let ann = ext
            .annotations
            .iter()
            .find(|a| a.kind == crate::annotate::AnnKind::Highlight)
            .expect("one highlight");
        assert_eq!(
            &ext.cleaned[ann.cleaned_content.start.raw()..ann.cleaned_content.end.raw()],
            "flat"
        );

        let cells = cell_products(&ext.cleaned);
        let (_tree, content_evs, plain) = &cells[0];
        assert_eq!(plain, "the earth is flat here");

        let local = crate::annotate::map_cleaned_highlight_to_local(
            &ext.cleaned,
            ann.cleaned_content.start.raw(),
            ann.cleaned_content.end.raw(),
            content_evs,
        );
        assert_eq!(local, vec![(13, 17)], "cell-local char range of \"flat\"");

        let hl: Vec<(usize, usize)> = local
            .iter()
            .map(|&(a, b)| (a as usize, b as usize))
            .collect();
        let theme = crate::theme::active();
        let markup = crate::renderer::annotate_markup(plain, &hl, &theme);
        // Built from the same generator the code under test uses — this test is about
        // WHICH characters get highlighted, not what colour they get (the colour is
        // the active theme's, and `theme` owns testing its resolution).
        assert_eq!(
            markup,
            format!(
                "the earth is {}flat</span> here",
                crate::renderer::ann_hl_open(&theme)
            )
        );
    }

    #[test]
    fn xx_display_maps_bold_cell_claim_char_precisely() {
        // Annotation covers the bold claim; plain label is "see flat here" without **.
        let original = "| see {==**flat**==}{>>n<<} here | x |\n| --- | --- |\n| y | z |\n";
        let ext = crate::annotate::extract(original);
        let ann = ext
            .annotations
            .iter()
            .find(|a| a.kind == crate::annotate::AnnKind::Highlight)
            .expect("highlight");
        // cleaned keeps **flat** as the claim text inside the cell source.
        assert!(
            ext.cleaned[ann.cleaned_content.start.raw()..ann.cleaned_content.end.raw()]
                .contains("flat"),
            "cleaned claim: {:?}",
            &ext.cleaned[ann.cleaned_content.start.raw()..ann.cleaned_content.end.raw()]
        );

        let cells = cell_products(&ext.cleaned);
        let (_tree, content_evs, plain) = &cells[0];
        let local = crate::annotate::map_cleaned_highlight_to_local(
            &ext.cleaned,
            ann.cleaned_content.start.raw(),
            ann.cleaned_content.end.raw(),
            content_evs,
        );
        // "flat" is the rendered bold content — local range should cover those chars.
        let flat_a = plain_off(plain, "flat") as usize;
        let flat_b = flat_a + "flat".chars().count();
        assert!(
            local
                .iter()
                .any(|&(a, b)| a as usize <= flat_a && b as usize >= flat_b),
            "local ranges {local:?} must cover plain[{flat_a}..{flat_b}] = flat; plain={plain:?}"
        );
        let hl: Vec<(usize, usize)> = local
            .iter()
            .map(|&(a, b)| (a as usize, b as usize))
            .collect();
        let theme = crate::theme::active();
        let markup = crate::renderer::annotate_markup(plain, &hl, &theme);
        assert!(
            markup.contains(&crate::renderer::ann_hl_open(&theme)),
            "markup must carry the annotation highlight span: {markup}"
        );
        assert!(
            markup.contains("flat"),
            "claim text must remain visible: {markup}"
        );
    }
}
