//! Tests of `preview::annotate`, kept beside it to hold that file under the size limit.

use super::*;
use crate::codeview::CreateAnnotation;
use crate::copymap::{build, Construct, CopyTree, RawEv, RawKind};

/// A copymap for a single plain-text paragraph (no inline constructs).
fn plain_tree(md: &str) -> CopyTree {
    let n = md.chars().count() as i32;
    let evs = vec![
        RawEv {
            buf: crate::copymap::BufSpan::new(0, 0),
            src: 0..md.len(),
            kind: RawKind::Start(Construct::Paragraph),
        },
        RawEv {
            buf: crate::copymap::BufSpan::new(0, n),
            src: 0..md.len(),
            kind: RawKind::Text(md.to_string()),
        },
        RawEv {
            buf: crate::copymap::BufSpan::new(n, n),
            src: 0..md.len(),
            kind: RawKind::End(Construct::Paragraph),
        },
    ];
    build(
        md,
        &evs,
        n,
        &std::rc::Rc::new(crate::renderer::BlockScripts::scan(md)),
    )
}

/// A copymap for two plain paragraphs "para one\n\npara two" (the `\n\n` separator
/// sits in the inter-paragraph buffer gap, buf 8→10).
fn two_para_tree() -> (CopyTree, String) {
    two_para_tree_between("\n\n")
}

/// `para one`, `between`, `para two`, rendered as two paragraphs: the buffer holds
/// its own `\n\n` between them whatever separates them in the source.
fn two_para_tree_between(between: &str) -> (CopyTree, String) {
    let md = format!("para one{between}para two");
    let p2 = 8 + between.len();
    let evs = vec![
        RawEv {
            buf: crate::copymap::BufSpan::new(0, 0),
            src: 0..8,
            kind: RawKind::Start(Construct::Paragraph),
        },
        RawEv {
            buf: crate::copymap::BufSpan::new(0, 8),
            src: 0..8,
            kind: RawKind::Text("para one".into()),
        },
        RawEv {
            buf: crate::copymap::BufSpan::new(8, 8),
            src: 0..8,
            kind: RawKind::End(Construct::Paragraph),
        },
        RawEv {
            buf: crate::copymap::BufSpan::new(8, 10),
            src: p2..p2 + 8,
            kind: RawKind::Start(Construct::Paragraph),
        },
        RawEv {
            buf: crate::copymap::BufSpan::new(10, 18),
            src: p2..p2 + 8,
            kind: RawKind::Text("para two".into()),
        },
        RawEv {
            buf: crate::copymap::BufSpan::new(18, 18),
            src: p2..p2 + 8,
            kind: RawKind::End(Construct::Paragraph),
        },
    ];
    let scripts = std::rc::Rc::new(crate::renderer::BlockScripts::scan(&md));
    (build(&md, &evs, 18, &scripts), md)
}

/// The source range an editor selection commits a highlight over — the shape most
/// of the balancing cases below assert on, now that a create carries a re-resolvable
/// reference rather than a range.
fn editor_highlight(src: &str, a: i32, b: i32) -> Option<std::ops::Range<usize>> {
    match capture_editor_selection(src, a, b)?.with_comment("note")? {
        CreateAnnotation::Highlight { target, .. } => target.resolve(src),
        CreateAnnotation::Point { .. } => None,
    }
}

#[test]
fn selection_within_a_block_becomes_a_highlight() {
    let cleaned = "the earth is flat";
    let tree = plain_tree(cleaned);
    let target = capture_selection(&tree, &[(0, 0)], cleaned, cleaned, 13, 17)
        .expect("an intra-block selection captures");
    assert_eq!(target.resolve(cleaned), Some(13..17));
    match target.with_comment("citation needed") {
        Some(CreateAnnotation::Highlight { target, comment }) => {
            assert_eq!(&cleaned[target.resolve(cleaned).unwrap()], "flat");
            assert_eq!(comment, "citation needed");
        }
        other => panic!("expected a highlight, got {other:?}"),
    }
}

/// The whole point of capturing early: the card is open while the document moves.
#[test]
fn a_captured_target_follows_its_text_through_an_edit_above_it() {
    let cleaned = "the earth is flat";
    let tree = plain_tree(cleaned);
    let target = capture_selection(&tree, &[(0, 0)], cleaned, cleaned, 13, 17).unwrap();
    let live = format!("PREPENDED {cleaned}");
    let at = target.resolve(&live).expect("the claim is still there");
    assert_eq!(&live[at], "flat");
    // ...and a claim that is GONE resolves to nothing, which the mutation makes a
    // clean no-op rather than a splice at coordinates naming something else.
    assert_eq!(target.resolve("the earth is round"), None);
}

#[test]
fn empty_comment_is_rejected() {
    let cleaned = "the earth is flat";
    let tree = plain_tree(cleaned);
    let target = capture_selection(&tree, &[(0, 0)], cleaned, cleaned, 13, 17).unwrap();
    assert!(target.with_comment("   ").is_none());
}

#[test]
fn highlight_range_translates_through_a_nonidentity_shift_table() {
    // cleaned "x flat" from an original where the kept text starts +3 (a "{=="
    // removed at cleaned 2): cleaned offset >= 2 maps to original + 3.
    let cleaned = "x flat";
    let tree = plain_tree(cleaned);
    let shifts = [(0usize, 0usize), (2usize, 5usize)];
    let original = "x {==flat==}";
    let target = capture_selection(&tree, &shifts, cleaned, original, 2, 6).unwrap();
    assert_eq!(target.resolve(original), Some(5..9));
}

#[test]
fn entry_card_is_centered_above_the_anchor_when_there_is_room() {
    // Anchor (midpoint x = 400) mid-viewport: the card centers on it and sits wholly
    // ABOVE the anchor line, as the Annotate card it replaces does.
    let (x, y) = bar_placement(400, 200, 18, 340, 40, 800, 600);
    assert_eq!(x, 400 - 340 / 2, "centered horizontally on the anchor");
    assert!(
        (0..200 - 40).contains(&y),
        "placed above the selection line: {y}"
    );
}

#[test]
fn entry_card_falls_back_below_when_there_is_no_room_above() {
    // Anchor near the very top: no room for the card above, so it goes BELOW the
    // selection's first line.
    let (_x, y) = bar_placement(400, 10, 18, 340, 40, 800, 600);
    assert!(y > 10 + 18, "below the selection line: {y}");
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

/// The entry card replaces the Annotate card in its spot, so it sits exactly where a
/// `FloatingCard` pointing at the same line would: one gap, one flip, one clamp.
#[test]
fn entry_card_sits_where_a_floating_card_would() {
    use crate::widgets::floating_card::{place, Rect, Side};
    for (ax, ay) in [(400, 200), (400, 10), (900, 300), (5, 590), (400, 44)] {
        let anchor = Rect {
            x: ax,
            y: ay,
            w: 0,
            h: 18,
        };
        let p = place(anchor, 340, 40, 800, 600, Side::Top);
        assert_eq!(
            bar_placement(ax, ay, 18, 340, 40, 800, 600),
            (p.x, p.y),
            "anchor ({ax}, {ay})"
        );
    }
}

#[test]
fn editor_selection_within_a_block_becomes_a_byte_range_highlight() {
    let src = "the earth is flat here";
    let target = capture_editor_selection(src, 13, 17).expect("captures");
    assert!(!target.is_point());
    match target.with_comment("citation needed") {
        Some(CreateAnnotation::Highlight { target, comment }) => {
            assert_eq!(target.resolve(src), Some(13..17));
            assert_eq!(&src[target.resolve(src).unwrap()], "flat");
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
    let target = capture_editor_selection(src, 5, 9).expect("captures");
    assert_eq!(target.resolve(src), Some(6..10));
    assert_eq!(&src[target.resolve(src).unwrap()], "flat");
}

#[test]
fn editor_selection_crossing_a_blank_line_becomes_a_point_comment_at_the_end() {
    let src = "para one\n\npara two";
    // chars 5..15 span the blank-line separator.
    let target = capture_editor_selection(src, 5, 15).expect("captures");
    assert!(
        target.is_point(),
        "a cross-block selection is a point comment"
    );
    match target.with_comment("spans blocks") {
        Some(CreateAnnotation::Point { target, comment }) => {
            assert_eq!(
                target.resolve(src).map(|r| r.end),
                Some(15),
                "point comment anchors at the selection end (byte)"
            );
            assert_eq!(comment, "spans blocks");
        }
        other => panic!("expected a point comment, got {other:?}"),
    }
}

/// The editor card must offer back the comment its commit is about to overwrite.
///
/// The editor-side equivalent of the preview's pre-population. The card
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
    let create = capture_editor_selection(src, a, b)
        .and_then(|t| t.with_comment(&shown))
        .expect("the pre-populated comment must commit");
    let CreateAnnotation::Highlight { target, comment } = create else {
        panic!("expected a highlight create");
    };
    let out = crate::annotate::insert_or_extend_highlight(
        src,
        target.resolve(src).expect("the claim is still there"),
        &comment,
    );

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
        let via_commit = match capture_editor_selection(src, a, b).and_then(|t| t.with_comment("x"))
        {
            Some(CreateAnnotation::Highlight { target, .. }) => target.resolve(src),
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
    let commit =
        |a, b, c: &str| capture_editor_selection(src, a, b).and_then(|t| t.with_comment(c));
    assert!(commit(13, 17, "   ").is_none());
    assert!(commit(13, 13, "x").is_none());
    // Reversed offsets are normalised, not rejected.
    assert!(commit(17, 13, "x").is_some());
}

// ── an editor annotation must not split an inline construct ──

#[test]
fn ww_editor_selection_inside_strong_wraps_the_whole_construct() {
    // The exact case: selecting "bol" inside `**bold**` used to
    // produce `**{==bol==}{>>note<<}d**`, splitting the emphasis run.
    let src = "a **bold** b";
    // chars 4..7 = "bol", strictly inside the `**` delimiters.
    match editor_highlight(src, 4, 7) {
        Some(range) => {
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
    match editor_highlight(src, 13, 17) {
        Some(range) => {
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
    match editor_highlight(src, 2, 10) {
        Some(range) => assert_eq!(range, 2..10),
        other => panic!("expected a highlight, got {other:?}"),
    }
}

#[test]
fn ww_editor_selection_straddling_a_code_span_swallows_it_whole() {
    // A code span is atomic — splitting its backticks breaks the construct.
    let src = "run `cargo test` now";
    // chars 5..11 = "cargo " — starts inside the code span's content.
    match editor_highlight(src, 5, 11) {
        Some(range) => {
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
    match editor_highlight(src, 6, 12) {
        Some(range) => {
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
    match editor_highlight(src, 8, 11) {
        Some(range) => {
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
        match editor_highlight(src, s as i32, (s + sub.len()) as i32) {
            Some(range) => {
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
    match editor_highlight(src, from as i32, to as i32) {
        Some(range) => {
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
        match editor_highlight(src, s as i32, (s + sub.len()) as i32) {
            Some(range) => {
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
    let target = capture_editor_selection(src, 5, 15).expect("captures");
    assert!(
        target.is_point(),
        "a span crossing a blank line is a point comment"
    );
    assert_eq!(target.resolve(src).map(|r| r.end), Some(15));
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
            buf: crate::copymap::BufSpan::new(0, 0),
            src: 0..p1_end,
            kind: RawKind::Start(Construct::Paragraph),
        },
        RawEv {
            buf: crate::copymap::BufSpan::new(0, p1_end as i32),
            src: 0..p1_end,
            kind: RawKind::Text(cleaned[..p1_end].to_string()),
        },
        RawEv {
            buf: crate::copymap::BufSpan::new(p1_end as i32, p1_end as i32),
            src: 0..p1_end,
            kind: RawKind::End(Construct::Paragraph),
        },
        RawEv {
            buf: crate::copymap::BufSpan::new(p1_end as i32, p2_start as i32),
            src: p2_start..cleaned.len(),
            kind: RawKind::Start(Construct::Paragraph),
        },
        RawEv {
            buf: crate::copymap::BufSpan::new(p2_start as i32, n),
            src: p2_start..cleaned.len(),
            kind: RawKind::Text(cleaned[p2_start..].to_string()),
        },
        RawEv {
            buf: crate::copymap::BufSpan::new(n, n),
            src: p2_start..cleaned.len(),
            kind: RawKind::End(Construct::Paragraph),
        },
    ];
    let scripts = std::rc::Rc::new(crate::renderer::BlockScripts::scan(&cleaned));
    (
        build(&cleaned, &evs, n, &scripts),
        ext.shifts,
        cleaned,
        original,
    )
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
    let Some(SelectionTarget::Point(span)) = selection_target(&tree, &shifts, &cleaned, a, b)
    else {
        panic!("a cross-block selection must resolve to a Point");
    };
    // The point comment anchors at the selection's END.
    let at = span.end;
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
    let out = crate::annotate::insert_point_comment(
        &original,
        &crate::annotate::Slot::bare(safe),
        "my comment",
    );

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
    let target = capture_selection(&tree, &[(0, 0)], &cleaned, &cleaned, 5, 15)
        .expect("a cross-block selection captures");
    assert!(target.is_point());
    match target.with_comment("spans blocks") {
        Some(CreateAnnotation::Point { target, comment }) => {
            assert_eq!(
                target.resolve(&cleaned).map(|r| r.end),
                Some(15),
                "point comment anchors at the selection end"
            );
            assert_eq!(comment, "spans blocks");
        }
        other => panic!("expected a point comment, got {other:?}"),
    }
}

/// A selection is a point comment exactly where extraction would refuse the
/// highlight: across a blank line in either line ending, or a line of spaces. Tested
/// for `\n\n` alone, a CRLF file's cross-paragraph selection was written as a
/// highlight that then rendered as literal markup.
#[test]
fn a_selection_across_any_blank_line_is_a_point_comment() {
    for between in ["\n\n", "\r\n\r\n", "\n \t\n", "\r\n  \r\n"] {
        let (tree, cleaned) = two_para_tree_between(between);
        let shifts = [(0, 0)];
        assert!(
            matches!(
                selection_target(&tree, &shifts, &cleaned, 5, 15),
                Some(SelectionTarget::Point(_))
            ),
            "preview, {between:?}"
        );
        let end = cleaned.chars().count() as i32;
        assert!(
            matches!(
                editor_selection_target(&cleaned, 5, end),
                Some(SelectionTarget::Point(_))
            ),
            "editor, {between:?}"
        );
        assert!(
            matches!(
                editor_selection_target(&cleaned, 0, 8),
                Some(SelectionTarget::Highlight(_))
            ),
            "one paragraph is still a highlight, {between:?}"
        );
    }
    // A list item's blank line, and a quote's (a line of only its `>` markers).
    for src in [
        "- one\n  \n  two",
        "> one\n>\n> two",
        "> one\r\n> \r\n> two",
    ] {
        assert!(
            matches!(
                editor_selection_target(src, 2, src.chars().count() as i32),
                Some(SelectionTarget::Point(_))
            ),
            "{src:?}"
        );
    }
}

// ── table-cell annotation validation: cell copymap + capture_selection composition ──
//
// Mirrors `preview/build.rs` cell capture: buf is 0-based cell-local, src is
// cleaned-document-absolute. NO mocks — real `copymap::{build,classify,cell_width}`
// and real `capture_selection` / `annotate::extract`.

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
    let scripts = std::rc::Rc::new(crate::renderer::BlockScripts::scan(cleaned));
    for (ev, src) in Parser::new_ext(cleaned, crate::renderer::md_options()).into_offset_iter() {
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
                out.push((
                    build(cleaned, &evs, off, &scripts),
                    content.clone(),
                    plain.clone(),
                ));
                active = false;
            }
            // dispatch-selector: selects every event while inside the cell
            // (`active`), not by which Event/Tag/TagEnd variant arrived —
            // `classify(&ev)` above is the real, already-exhaustive dispatcher;
            // this arm only decides whether an already-classified event belongs
            // to the cell being built (mirrors copymap::tests::cell_trees).
            _ if active => {
                if let Some(k) = &kind {
                    let w = crate::copymap::cell_width(&scripts, src.start, k);
                    let before = off;
                    let after = off + w;
                    evs.push(RawEv {
                        buf: crate::copymap::BufSpan::new(before, after),
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
            // dispatch-selector: sibling of the `_ if active` arm above — document
            // content outside any table cell is irrelevant to a per-cell product
            // builder, regardless of its variant, so this discards on the same
            // `active` axis, not on identity.
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
fn xx_cell_selection_maps_to_original_via_capture_selection() {
    // Identity shifts (no prior CriticMarkup): cell-local "flat" → original "flat".
    let original = "| the earth is flat here | x |\n| --- | --- |\n| y | z |\n";
    let ext = crate::annotate::extract(original);
    let cells = cell_products(&ext.cleaned);
    // Header row cell 0: plain "the earth is flat here"
    let (tree, _evs, plain) = &cells[0];
    assert_eq!(plain, "the earth is flat here");
    let a = plain_off(plain, "flat");
    let b = a + "flat".chars().count() as i32;
    match capture_selection(tree, &ext.shifts, &ext.cleaned, original, a, b)
        .and_then(|t| t.with_comment("citation needed"))
    {
        Some(CreateAnnotation::Highlight { target, comment }) => {
            let range = target
                .resolve(original)
                .expect("the claim is in the source");
            assert_eq!(&original[range], "flat");
            assert_eq!(comment, "citation needed");
        }
        other => panic!("expected highlight, got {other:?}"),
    }
}

#[test]
fn xx_cell_selection_translates_through_real_shift_table() {
    // CriticMarkup BEFORE the table shifts cleaned→original; the cell copymap is
    // built against cleaned, so capture_selection must compose both maps.
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
    match capture_selection(tree, &ext.shifts, &ext.cleaned, original, a, b)
        .and_then(|t| t.with_comment("cite"))
    {
        Some(CreateAnnotation::Highlight { target, comment }) => {
            let range = target
                .resolve(original)
                .expect("the claim is in the source");
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
    match capture_selection(tree, &ext.shifts, &ext.cleaned, original, a, b)
        .and_then(|t| t.with_comment("whole"))
    {
        Some(CreateAnnotation::Highlight { target, .. }) => {
            let range = target
                .resolve(original)
                .expect("the claim is in the source");
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
