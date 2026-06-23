//! Annotations viewer model.
//!
//! A *display-free* extraction of the document's comment-bearing annotations
//! (CriticMarkup comments) for the annotations sidebar — the direct analogue of
//! `outline::extract_headings` for headings. Kept free of any GTK type so it can be
//! unit-tested under POLICY's no-live-display coverage gate.
//!
//! **Identity, not index.** Each entry carries its `src_span` (its byte span in the
//! ORIGINAL source), and navigation keys on that span — never on the entry's
//! position in the list. The list is a *filtered subsequence* of every CriticMarkup
//! construct (bare highlights and the inert suggested-edit kinds are excluded, TDD
//! 20.2), so a row's position is generally **not** its margin chip's position. A
//! viewer that navigated by row index would silently open the wrong annotation's
//! popover on any document containing an excluded construct (GTK4Rs/AP-74).

use crate::annotate::{extract, AnnKind};
use crate::span::OriginalByteOffset;
use std::ops::Range;

/// One annotation in document order (TDD 20.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AnnotationEntry {
    /// Full construct span in the **original** source — the identity key the viewer
    /// navigates by (resolved to a margin-chip index by the marker layer). Stable and
    /// unique; also the mutation path's identity (Remove/Edit address by byte span).
    /// Typed [`OriginalByteOffset`] so it can never be confused with a buffer char
    /// offset or a cleaned-source byte (the source of past displacement bugs).
    pub(crate) src_span: Range<OriginalByteOffset>,
    /// Which CriticMarkup construct this is (`Highlight` with a comment, or a point
    /// `Comment`) — the only two kinds `is_listed` admits.
    pub(crate) kind: AnnKind,
    /// The comment body text. Always non-empty for a listed annotation (a listed
    /// annotation is comment-bearing by definition).
    pub(crate) comment: String,
    /// The highlighted claim snippet (shown dimmed as secondary text) for a
    /// `Highlight`; `None` for a point comment, which annotates a position, not a span.
    pub(crate) claim: Option<String>,
}

/// Extract the document's comment-bearing annotations, in document order.
///
/// Membership is exactly [`crate::annotate::Annotation::is_listed`] — the SAME
/// predicate the preview's margin chips use — so the list and the chips can never
/// disagree about what is an annotation (TDD 20.2). The claim snippet is read from
/// the *cleaned* text (the highlighted content range), matching how `build_markers`
/// derives the chip's claim, so the two presentations stay identical.
///
/// An empty document — or one with no comment-bearing annotations — yields an empty
/// vector (the panel renders that as a muted "No annotations" placeholder, not an
/// error — TDD 20.3).
pub(crate) fn extract_entries(source: &str) -> Vec<AnnotationEntry> {
    let ex = extract(source);
    ex.annotations
        .iter()
        .filter(|a| a.is_listed())
        .map(|a| AnnotationEntry {
            // `Annotation::src_span` is now itself a `Range<OriginalByteOffset>`
            // (N1-phase-2), so this producer boundary is a straight clone — no
            // re-wrapping, and a cleaned offset here would not compile.
            src_span: a.src_span.clone(),
            kind: a.kind,
            // `is_listed` guarantees `comment.is_some()`.
            comment: a.comment.clone().unwrap_or_default(),
            claim: match a.kind {
                AnnKind::Highlight => Some(
                    ex.cleaned[a.cleaned_content.start.raw()..a.cleaned_content.end.raw()]
                        .to_string(),
                ),
                _ => None,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_highlight_and_point_comments_in_order() {
        let md = "A {==claim==}{>>note one<<} and later {>>a point<<} end.";
        let es = extract_entries(md);
        assert_eq!(es.len(), 2);
        assert_eq!(es[0].kind, AnnKind::Highlight);
        assert_eq!(es[0].comment, "note one");
        assert_eq!(es[0].claim.as_deref(), Some("claim"));
        assert_eq!(es[1].kind, AnnKind::Comment);
        assert_eq!(es[1].comment, "a point");
        assert_eq!(es[1].claim, None);
        // Document order: spans strictly increasing.
        assert!(es[0].src_span.start < es[1].src_span.start);
    }

    #[test]
    fn no_annotations_yields_empty() {
        assert!(extract_entries("").is_empty());
        assert!(extract_entries("Just prose, no annotations.").is_empty());
    }

    #[test]
    fn a_bare_highlight_is_not_listed() {
        // A comment-less highlight is a rendering feature, not an annotation (TDD 20.2).
        let es = extract_entries("The sky is {==blue==} today.");
        assert!(es.is_empty());
    }

    #[test]
    fn inert_kinds_are_not_listed() {
        // Insertion / deletion / substitution have no chip and no viewer row (TDD 20.2).
        let md = "a {++ins++} b {--del--} c {~~old~>new~~} d";
        assert!(extract_entries(md).is_empty());
    }

    #[test]
    fn a_bare_highlight_before_a_comment_does_not_shift_membership() {
        // The membership-vs-position hazard: the
        // bare highlight is skipped, so the ONE listed entry is the commented point,
        // and its src_span identity — not its list position — is what will navigate.
        let md = "x {==bare==} y {>>real note<<} z";
        let es = extract_entries(md);
        assert_eq!(es.len(), 1);
        assert_eq!(es[0].comment, "real note");
        // The identity span points at the real annotation, past the bare highlight.
        assert!(md[es[0].src_span.start.raw()..es[0].src_span.end.raw()].starts_with("{>>"));
    }

    #[test]
    fn claim_snippet_is_the_highlighted_text() {
        let es = extract_entries("see {==the quick brown fox==}{>>nice<<} here");
        assert_eq!(es.len(), 1);
        assert_eq!(es[0].claim.as_deref(), Some("the quick brown fox"));
    }
}
