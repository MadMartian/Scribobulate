//! Writing suggested edits: a new addition, replacement or deletion over the reader's
//! selection (TDD 17.79), and rewriting the side an existing one would add (TDD 17.78).
//!
//! The text the reader types is not a comment, so [`super::mutate::sanitize_comment`]
//! does not cover it: what could end it early is the construct's own closer (`++}` for an
//! addition, `~~}` for a replacement), and a line break, which the scanner refuses inside
//! a construct once it makes a blank line (the construct would then read as literal
//! text). A suggestion is inline text, so every line break becomes a space.

use super::scan::{DEL_CLOSE, DEL_OPEN, INS_CLOSE, INS_OPEN, SUB_ARROW, SUB_CLOSE, SUB_OPEN};
use std::ops::Range;

/// The closer of the construct that starts at `construct_start` in `source`, for the two
/// kinds whose added side the reader can write; `None` for every other construct.
fn added_side_closer(source: &str, construct_start: usize) -> Option<&'static str> {
    let head = source.get(construct_start..)?;
    if head.starts_with(INS_OPEN) {
        Some(INS_CLOSE)
    } else if head.starts_with(SUB_OPEN) {
        Some(SUB_CLOSE)
    } else {
        None
    }
}

/// `text` made safe to sit inside a construct closed by `closer`: line breaks become
/// spaces and the closer is broken with a space so it no longer closes anything.
pub(crate) fn sanitize_suggestion(text: &str, closer: &str) -> String {
    let flat: String = text
        .replace("\r\n", " ")
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    let (head, tail) = closer.split_at(closer.len() - 1);
    flat.replace(closer, &format!("{head} {tail}"))
}

/// Rewrite the added side `side` of the suggestion starting at `construct_start` with
/// `text`. `None` when the construct is not an addition or a replacement, when `side` is
/// not a slice of `source`, or when an addition would be left empty — an addition of
/// nothing is not a suggestion; removing it is Remove's job. A replacement may propose
/// nothing (`{~~old~>~~}`), which reads as a deletion.
pub(crate) fn edit_added_side(
    source: &str,
    construct_start: usize,
    side: Range<usize>,
    text: &str,
) -> Option<String> {
    let closer = added_side_closer(source, construct_start)?;
    let clean = sanitize_suggestion(text, closer);
    if closer == INS_CLOSE && clean.trim().is_empty() {
        return None;
    }
    let head = source.get(..side.start)?;
    let tail = source.get(side.end..)?;
    Some(format!("{head}{clean}{tail}"))
}

/// Which suggested edit a reader asked to write over their selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SuggestKind {
    /// Text to add straight after the selection: `{++text++}`.
    Add,
    /// The selection, replaced: `{~~selection~>text~~}`.
    Replace,
    /// The selection, deleted: `{--selection--}`.
    Delete,
}

impl SuggestKind {
    /// The action target naming this kind (`win.suggest::add`), parsed back by
    /// [`Self::from_target`].
    pub(crate) fn target(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Replace => "replace",
            Self::Delete => "delete",
        }
    }

    /// The kind an action target names; `None` for anything else.
    pub(crate) fn from_target(target: &str) -> Option<Self> {
        [Self::Add, Self::Replace, Self::Delete]
            .into_iter()
            .find(|k| k.target() == target)
    }
}

/// Where a new suggestion over the selection `range` may be written: the range itself,
/// widened as a highlight's would be over a code span or formula it splits
/// ([`super::landing::for_highlight`]). `None` when it may not be — a selection that
/// reaches into code, front matter, an HTML block or math, or that touches an existing
/// annotation: CriticMarkup does not nest, so a suggestion over a mark would be read as
/// literal text by every other tool. Also `None` for an empty selection.
pub(crate) fn landing(source: &str, range: Range<usize>) -> Option<Range<usize>> {
    let super::Landing::Highlight(range) = super::landing::for_highlight(source, range) else {
        return None;
    };
    let touches_mark = super::extract(source).annotations.iter().any(|m| {
        let (s, e) = (m.src_span.start.raw(), m.src_span.end.raw());
        s < range.end && range.start < e
    });
    (!range.is_empty() && !touches_mark).then_some(range)
}

/// Write a new suggestion of `kind` over the selection `range` of `source`, with `text`
/// as what an addition or replacement proposes. `None` (nothing written) when the
/// selection may not take one ([`landing`]), when an addition proposes nothing, or when
/// the selection itself holds a delimiter the construct would close on — selected text
/// is written verbatim, never altered.
pub(crate) fn insert_suggestion(
    source: &str,
    range: Range<usize>,
    kind: SuggestKind,
    text: &str,
) -> Option<String> {
    let range = landing(source, range)?;
    let selected = source.get(range.clone())?;
    let (at, construct) = match kind {
        SuggestKind::Add => {
            let clean = sanitize_suggestion(text, INS_CLOSE);
            if clean.trim().is_empty() {
                return None;
            }
            (
                range.end..range.end,
                format!("{INS_OPEN}{clean}{INS_CLOSE}"),
            )
        }
        SuggestKind::Replace => {
            if selected.contains(SUB_ARROW) || selected.contains(SUB_CLOSE) {
                return None;
            }
            let clean = sanitize_suggestion(text, SUB_CLOSE);
            (
                range,
                format!("{SUB_OPEN}{selected}{SUB_ARROW}{clean}{SUB_CLOSE}"),
            )
        }
        SuggestKind::Delete => {
            if selected.contains(DEL_CLOSE) {
                return None;
            }
            (range, format!("{DEL_OPEN}{selected}{DEL_CLOSE}"))
        }
    };
    let mut out = String::with_capacity(source.len() + construct.len());
    out.push_str(&source[..at.start]);
    out.push_str(&construct);
    out.push_str(&source[at.end..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotate::{extract, AnnKind};

    /// The added side of the only construct in `src`, as the scanner reports it.
    fn added_side(src: &str) -> (usize, Range<usize>) {
        let ann = extract(src).annotations.remove(0);
        let sides = crate::annotate::suggestion_sides(&ann).expect("a suggestion");
        (ann.src_span.start.raw(), sides.new.expect("an added side"))
    }

    #[test]
    fn an_additions_text_is_rewritten_in_place() {
        let src = "Say {++hello++}{>>greet<<} now.";
        let (at, side) = added_side(src);
        assert_eq!(
            edit_added_side(src, at, side, "**good** day").as_deref(),
            Some("Say {++**good** day++}{>>greet<<} now.")
        );
    }

    #[test]
    fn a_replacements_new_side_is_rewritten_and_its_old_side_kept() {
        let src = "A {~~cat~>dog~~} sat.";
        let (at, side) = added_side(src);
        let out = edit_added_side(src, at, side, "fox").unwrap();
        assert_eq!(out, "A {~~cat~>fox~~} sat.");
        let ann = &extract(&out).annotations[0];
        assert_eq!(ann.kind, AnnKind::Substitution);
        assert_eq!(&out[ann.src_old.clone().unwrap()], "cat");
    }

    #[test]
    fn a_replacement_may_propose_nothing_but_an_addition_may_not() {
        let src = "A {~~cat~>dog~~} sat.";
        let (at, side) = added_side(src);
        assert_eq!(
            edit_added_side(src, at, side, "").as_deref(),
            Some("A {~~cat~>~~} sat.")
        );
        let src = "A {++dog++} sat.";
        let (at, side) = added_side(src);
        assert_eq!(edit_added_side(src, at, side, "  "), None);
    }

    #[test]
    fn typed_closers_and_line_breaks_cannot_end_the_construct() {
        for (src, typed) in [
            ("A {++dog++} sat.", "x++}y\n\nz"),
            ("A {~~cat~>dog~~} sat.", "x~~}y\r\n\r\nz"),
        ] {
            let (at, side) = added_side(src);
            let out = edit_added_side(src, at, side, typed).unwrap();
            let marks = extract(&out).annotations;
            assert_eq!(marks.len(), 1, "{out:?} is still one construct");
            assert_eq!(
                marks[0].src_span.end.raw(),
                out.len() - " sat.".len(),
                "{out:?}"
            );
        }
    }

    #[test]
    fn only_an_addition_or_a_replacement_has_an_added_side_to_write() {
        let src = "A {--dog--} sat.";
        assert_eq!(edit_added_side(src, 2, 5..8, "x"), None);
        assert_eq!(edit_added_side("A {++dog++}", 2, 5..99, "x"), None);
    }

    /// The range of `needle`'s first occurrence in `src`.
    fn sel(src: &str, needle: &str) -> Range<usize> {
        let at = src.find(needle).unwrap();
        at..at + needle.len()
    }

    #[test]
    fn each_kind_is_written_over_the_selection() {
        let src = "The cat sat.\n";
        let r = sel(src, "cat");
        assert_eq!(
            insert_suggestion(src, r.clone(), SuggestKind::Add, " black").as_deref(),
            Some("The cat{++ black++} sat.\n")
        );
        assert_eq!(
            insert_suggestion(src, r.clone(), SuggestKind::Replace, "dog").as_deref(),
            Some("The {~~cat~>dog~~} sat.\n")
        );
        assert_eq!(
            insert_suggestion(src, r, SuggestKind::Delete, "").as_deref(),
            Some("The {--cat--} sat.\n")
        );
    }

    #[test]
    fn a_written_suggestion_reads_back_as_that_kind() {
        let src = "The cat sat.\n";
        for (kind, ann) in [
            (SuggestKind::Add, AnnKind::Insertion),
            (SuggestKind::Replace, AnnKind::Substitution),
            (SuggestKind::Delete, AnnKind::Deletion),
        ] {
            let out = insert_suggestion(src, sel(src, "cat"), kind, "x~~}y++}z").unwrap();
            let marks = extract(&out).annotations;
            assert_eq!(marks.len(), 1, "{out:?}");
            assert_eq!(marks[0].kind, ann, "{out:?}");
        }
    }

    #[test]
    fn no_suggestion_is_written_over_an_annotation_code_or_nothing() {
        let src = "A {==claim==}{>>c<<} and `code` here.\n\n```\nfenced\n```\n";
        for needle in ["claim", "A {==cl", "c<<", "fenced"] {
            for kind in [SuggestKind::Add, SuggestKind::Replace, SuggestKind::Delete] {
                assert_eq!(
                    insert_suggestion(src, sel(src, needle), kind, "x"),
                    None,
                    "{kind:?} over {needle:?}"
                );
            }
        }
        assert_eq!(insert_suggestion(src, 2..2, SuggestKind::Delete, ""), None);
        assert_eq!(
            insert_suggestion("a b\n", 0..1, SuggestKind::Add, " "),
            None
        );
    }

    #[test]
    fn selected_text_holding_the_closer_is_left_alone() {
        let src = "a --} b ~> c\n";
        assert_eq!(
            insert_suggestion(src, sel(src, "a --}"), SuggestKind::Delete, ""),
            None
        );
        assert_eq!(
            insert_suggestion(src, sel(src, "b ~>"), SuggestKind::Replace, "x"),
            None
        );
    }

    #[test]
    fn a_kind_round_trips_through_its_action_target() {
        for kind in [SuggestKind::Add, SuggestKind::Replace, SuggestKind::Delete] {
            assert_eq!(SuggestKind::from_target(kind.target()), Some(kind));
        }
        assert_eq!(SuggestKind::from_target("comment"), None);
    }
}
