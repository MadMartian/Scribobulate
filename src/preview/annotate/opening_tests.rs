//! What a card does for each kind it is raised for (`PendingTarget::opening`), and what
//! it writes (TDD 17.79). Through `capture_editor_selection`, whose char offsets are the
//! source's own, so each case reads as the selection a reader makes.

use super::*;
use crate::annotate::suggest::SuggestKind;
use crate::codeview::CreateAnnotation;
use crate::winstate::statusbar::{COMMENT_NOT_ANNOTATABLE, SUGGESTION_NOT_PLACEABLE};

/// The target over `needle`'s first occurrence in `src` (ASCII fixtures: chars = bytes).
fn over(src: &str, needle: &str) -> PendingTarget {
    let at = src.find(needle).unwrap() as i32;
    capture_editor_selection(src, at, at + needle.len() as i32).expect("a target")
}

const fn suggest(kind: SuggestKind) -> AnnotateKind {
    AnnotateKind::Suggest(kind)
}

/// The card's prefill and hint, or a panic naming what happened instead.
fn card(opening: Opening) -> (String, &'static str) {
    match opening {
        Opening::Card { prefill, hint, .. } => (prefill, hint),
        Opening::Refused(why) => panic!("refused: {why}"),
        Opening::Write(_) => panic!("written without a card"),
    }
}

#[test]
fn a_comment_card_opens_empty_or_with_what_it_merges() {
    let src = "The cat sat.\n";
    assert_eq!(
        card(over(src, "cat").opening(AnnotateKind::Comment, src)),
        (String::new(), COMMENT_HINT)
    );
    let src = "The {==cat==}{>>old<<} sat.\n";
    let (prefill, _) = card(over(src, "The {==cat").opening(AnnotateKind::Comment, src));
    assert_eq!(prefill, "old");
}

#[test]
fn add_opens_empty_and_replace_opens_on_the_text_as_written() {
    let src = "A **bold** word.\n";
    let (prefill, add_hint) = card(over(src, "word").opening(suggest(SuggestKind::Add), src));
    assert_eq!(prefill, "");
    let (prefill, replace_hint) =
        card(over(src, "bol").opening(suggest(SuggestKind::Replace), src));
    assert_eq!(
        prefill, "**bold**",
        "the whole construct, delimiters and all"
    );
    assert!(add_hint != replace_hint && replace_hint != COMMENT_HINT);
}

#[test]
fn delete_is_written_at_once() {
    let src = "The cat sat.\n";
    match over(src, "cat").opening(suggest(SuggestKind::Delete), src) {
        Opening::Write(CreateAnnotation::Suggest { target, kind, .. }) => {
            assert_eq!(kind, SuggestKind::Delete);
            assert_eq!(target.resolve(src), Some(4..7));
        }
        _ => panic!("a deletion writes without a card"),
    }
}

#[test]
fn a_suggestion_is_refused_across_blocks_over_annotations_and_in_code() {
    for (src, needle) in [
        ("One para.\n\nTwo para.\n", "para.\n\nTwo"),
        ("The {==cat==}{>>c<<} sat.\n", "The {==cat"),
        ("Text.\n\n```\ncode\n```\n", "code"),
    ] {
        for kind in [SuggestKind::Add, SuggestKind::Replace, SuggestKind::Delete] {
            assert!(
                matches!(
                    over(src, needle).opening(suggest(kind), src),
                    Opening::Refused(SUGGESTION_NOT_PLACEABLE)
                ),
                "{kind:?} over {needle:?}"
            );
        }
    }
}

#[test]
fn a_comment_inside_a_comment_is_refused_with_its_own_notice() {
    let src = "The {==cat==}{>>a long note<<} sat.\n";
    assert!(matches!(
        over(src, "long").opening(AnnotateKind::Comment, src),
        Opening::Refused(COMMENT_NOT_ANNOTATABLE)
    ));
}

#[test]
fn what_is_typed_becomes_that_kind_of_create() {
    let src = "The cat sat.\n";
    match over(src, "cat").with_text(suggest(SuggestKind::Replace), "dog") {
        Some(CreateAnnotation::Suggest { kind, text, .. }) => {
            assert_eq!((kind, text.as_str()), (SuggestKind::Replace, "dog"));
        }
        _ => panic!("a replacement"),
    }
    assert!(matches!(
        over(src, "cat").with_text(AnnotateKind::Comment, "note"),
        Some(CreateAnnotation::Highlight { .. })
    ));
}
